use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use calce_data::auth::AuthConfig;
use calce_data::auth::api_key::ApiKeyCache;
use calce_data::loader;
use calce_data::market_data_store::MarketDataStore;
use calce_data::user_data_store::UserDataStore;
use sqlx::PgPool;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

mod auth;
mod error;
mod rate_limit;
mod routes;
mod state;

#[cfg(test)]
mod seed;

use state::AppState;

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(routes::explorer))
        .merge(routes::calc_routes())
        .merge(routes::user_routes())
        .merge(routes::organization_routes())
        .merge(routes::auth_routes())
        .merge(routes::api_key_routes())
        .layer(CorsLayer::very_permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn create_postgres_service() -> (MarketDataStore, UserDataStore, PgPool) {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://calce:calce@localhost:5433/calce".into());

    let pool = PgPool::connect(&database_url)
        .await
        .expect("failed to connect to database");

    tracing::info!("Backend: postgres ({database_url})");
    let (market_data, user_data) = loader::load_from_postgres(&pool)
        .await
        .expect("failed to bulk-load data");
    (market_data, user_data, pool)
}

#[cfg(feature = "njorda")]
fn create_njorda_cache_service() -> MarketDataStore {
    use std::time::Instant;

    use calce_integrations::njorda::{self, cache};

    let cache_path = cache::cache_path();
    let service_path = cache::service_cache_path();

    // Try loading pre-built service cache
    if cache::service_is_fresh(&service_path, &cache_path) {
        tracing::info!("Loading pre-built service from {}", service_path.display());
        let t0 = Instant::now();
        match cache::load_service(&service_path) {
            Ok(market_data) => {
                let mem_mb = market_data.approx_heap_bytes() / (1024 * 1024);
                tracing::info!(
                    "Service loaded in {:.2}s ({} instruments, {} prices, {} FX rates, ~{} MB)",
                    t0.elapsed().as_secs_f64(),
                    market_data.instrument_ids().len(),
                    market_data.price_count(),
                    market_data.fx_rate_count(),
                    mem_mb,
                );
                return MarketDataStore::from_memory(market_data);
            }
            Err(e) => {
                tracing::warn!("Service cache invalid, rebuilding: {e}");
            }
        }
    }

    // Load raw cache and build
    tracing::info!("Loading njorda cache from {}", cache_path.display());
    let t0 = Instant::now();
    let cached = cache::load_from_file(&cache_path).unwrap_or_else(|e| {
        eprintln!(
            "Failed to load njorda cache at {}: {e}",
            cache_path.display()
        );
        eprintln!("Run 'invoke njorda-fetch' first to create the cache.");
        std::process::exit(1);
    });
    tracing::info!(
        "Cache loaded in {:.2}s: {} instruments, {} prices, {} FX rates — building index...",
        t0.elapsed().as_secs_f64(),
        cached.metadata.instrument_count,
        cached.metadata.price_count,
        cached.metadata.fx_rate_count,
    );

    let t0 = Instant::now();
    let market_data = njorda::build_service(&cached).unwrap_or_else(|e| {
        eprintln!("Failed to build market data from cache: {e}");
        std::process::exit(1);
    });
    let mem_mb = market_data.approx_heap_bytes() / (1024 * 1024);
    tracing::info!(
        "Index built in {:.2}s ({} instruments, {} prices, {} FX rates, ~{} MB)",
        t0.elapsed().as_secs_f64(),
        market_data.instrument_ids().len(),
        market_data.price_count(),
        market_data.fx_rate_count(),
        mem_mb,
    );

    // Save for next startup
    let t0 = Instant::now();
    match cache::save_service(&service_path, &market_data) {
        Ok(()) => tracing::info!(
            "Service saved to {} in {:.2}s",
            service_path.display(),
            t0.elapsed().as_secs_f64()
        ),
        Err(e) => tracing::warn!("Failed to save service cache: {e}"),
    }

    tracing::info!(
        "Date range: {} to {} (fetched {})",
        cached.metadata.date_from,
        cached.metadata.date_to,
        cached.metadata.fetched_at.format("%Y-%m-%d %H:%M UTC"),
    );

    MarketDataStore::from_memory(market_data)
}

#[cfg(not(feature = "njorda"))]
fn create_njorda_cache_service() -> MarketDataStore {
    eprintln!("Error: njorda-cache backend requires the 'njorda' feature.");
    eprintln!("Build with: cargo run -p calce-api --features njorda");
    eprintln!("Or use: invoke api-njorda");
    std::process::exit(1);
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let backend = std::env::var("CALCE_BACKEND").unwrap_or_else(|_| "postgres".into());

    let (market_data, user_data, pool) = match backend.as_str() {
        "postgres" => {
            let (md, ud, pool) = create_postgres_service().await;
            (md, ud, Some(pool))
        }
        "njorda-cache" => {
            let md = create_njorda_cache_service();
            let ud = UserDataStore::new();
            (md, ud, None)
        }
        other => {
            eprintln!("Unknown CALCE_BACKEND: {other}");
            eprintln!("Options: postgres (default), njorda-cache");
            std::process::exit(1);
        }
    };

    let auth_config = AuthConfig::from_env();

    let state = AppState {
        market_data: Arc::new(market_data),
        user_data: Arc::new(user_data),
        pool,
        auth_config,
        api_key_cache: ApiKeyCache::new(),
        auth_rate_limiter: rate_limit::create_auth_rate_limiter(),
    };

    let app = build_router(state);

    let port = std::env::var("PORT").unwrap_or_else(|_| "35701".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");

    tracing::info!("Dev console: http://localhost:{port}");
    tracing::info!("Listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.expect("server error");
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use calce_data::auth::{Role, jwt};
    use http_body_util::BodyExt;
    use std::sync::LazyLock;
    use tower::ServiceExt;

    /// Shared test config so all tests use the same JWT keys.
    static TEST_AUTH_CONFIG: LazyLock<AuthConfig> = LazyLock::new(AuthConfig::test_default);

    fn test_state() -> AppState {
        let market_data = MarketDataStore::from_memory(seed::seed_market_data());
        let user_data = seed::seed_user_data();
        AppState {
            market_data: Arc::new(market_data),
            user_data: Arc::new(user_data),
            pool: None,
            auth_config: TEST_AUTH_CONFIG.clone(),
            api_key_cache: ApiKeyCache::new(),
            auth_rate_limiter: rate_limit::create_auth_rate_limiter(),
        }
    }

    /// Mint a test JWT for the given user and role.
    fn test_token(user_id: &str, role: &Role) -> String {
        jwt::encode_access_token(user_id, role, None, &TEST_AUTH_CONFIG.jwt_encoding_key).unwrap()
    }

    async fn get(uri: &str, headers: &[(&str, &str)]) -> (StatusCode, serde_json::Value) {
        let app = build_router(test_state());
        let mut req = Request::builder().uri(uri);
        for &(k, v) in headers {
            req = req.header(k, v);
        }
        let response = app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        (status, json)
    }

    fn auth_headers() -> Vec<(String, String)> {
        let token = test_token("alice", &Role::Admin);
        vec![("authorization".to_owned(), format!("Bearer {token}"))]
    }

    async fn get_authed(uri: &str) -> (StatusCode, serde_json::Value) {
        let headers = auth_headers();
        let refs: Vec<(&str, &str)> = headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        get(uri, &refs).await
    }

    #[tokio::test]
    async fn market_value_returns_positions_and_total() {
        let (status, body) =
            get_authed("/v1/users/alice/market-value?as_of_date=2025-03-14&base_currency=SEK")
                .await;

        assert_eq!(status, StatusCode::OK);
        assert!(body["data"]["total"]["amount"].is_number());
        assert!(body["data"]["positions"].is_array());
    }

    #[tokio::test]
    async fn market_value_missing_auth_returns_401() {
        let (status, _) = get(
            "/v1/users/alice/market-value?as_of_date=2025-03-14&base_currency=SEK",
            &[],
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn market_value_bad_currency_returns_400() {
        let (status, body) =
            get_authed("/v1/users/alice/market-value?as_of_date=2025-03-14&base_currency=NOPE")
                .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "BAD_REQUEST");
    }

    #[tokio::test]
    async fn portfolio_report_returns_mv_and_changes() {
        let (status, body) =
            get_authed("/v1/users/alice/portfolio?as_of_date=2025-03-14&base_currency=SEK").await;

        assert_eq!(status, StatusCode::OK);
        assert!(body["data"]["market_value"]["total"]["amount"].is_number());
        assert!(body["data"]["value_changes"]["daily"]["change"]["amount"].is_number());
    }

    #[tokio::test]
    async fn volatility_returns_result() {
        let (status, body) =
            get_authed("/v1/instruments/AAPL/volatility?as_of_date=2025-03-14&lookback_days=365")
                .await;

        assert_eq!(status, StatusCode::OK);
        assert!(body["annualized_volatility"].is_number());
        assert!(body["daily_volatility"].is_number());
        assert!(body["num_observations"].is_number());
        assert!(body["start_date"].is_string());
        assert!(body["end_date"].is_string());
    }

    #[tokio::test]
    async fn volatility_defaults_lookback_to_3_years() {
        let (status, body) =
            get_authed("/v1/instruments/AAPL/volatility?as_of_date=2025-03-14").await;

        assert_eq!(status, StatusCode::OK);
        assert!(body["annualized_volatility"].is_number());
    }

    #[tokio::test]
    async fn volatility_unknown_instrument_returns_error() {
        let (status, _) =
            get_authed("/v1/instruments/DOESNOTEXIST/volatility?as_of_date=2025-03-14").await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn volatility_missing_auth_returns_401() {
        let (status, _) = get(
            "/v1/instruments/AAPL/volatility?as_of_date=2025-03-14&lookback_days=365",
            &[],
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn data_stats_requires_auth() {
        let (status, _) = get("/v1/data/stats", &[]).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        let (status, body) = get_authed("/v1/data/stats").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["instrument_count"].is_number());
    }

    #[tokio::test]
    async fn data_instruments_requires_auth() {
        let (status, _) = get("/v1/data/instruments", &[]).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        let (status, body) = get_authed("/v1/data/instruments").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["items"].as_array().is_some());
        assert!(body["total"].is_number());
    }

    #[tokio::test]
    async fn data_users_requires_auth() {
        let (status, _) = get("/v1/data/users", &[]).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn instrument_prices_requires_auth() {
        let (status, _) = get(
            "/v1/data/instruments/AAPL/prices?from=2025-01-01&to=2025-03-14",
            &[],
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        let (status, body) =
            get_authed("/v1/data/instruments/AAPL/prices?from=2025-01-01&to=2025-03-14").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.as_array().is_some());
    }
}
