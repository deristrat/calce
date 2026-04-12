use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use calce_data::auth::AuthConfig;
use calce_data::auth::api_key::ApiKeyCache;
use calce_data::loader;
use calce_data::market_data_store::MarketDataStore;
use calce_data::user_data_store::UserDataStore;
use calce_datastructs::pubsub::PubSub;
use sqlx::PgPool;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

mod auth;
pub mod db_simulator;
mod error;
mod rate_limit;
mod routes;
pub mod simulator;
mod state;

#[cfg(test)]
mod seed;

use state::AppState;

fn build_router(state: AppState) -> Router {
    Router::new()
        .merge(routes::calc_routes())
        .merge(routes::user_routes())
        .merge(routes::organization_routes())
        .merge(routes::auth_routes())
        .merge(routes::api_key_routes())
        .merge(routes::simulator_routes())
        .merge(routes::db_simulator_routes())
        .merge(routes::event_routes())
        .layer(CorsLayer::very_permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[allow(clippy::expect_used)] // startup — server cannot operate without DB
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

#[allow(clippy::expect_used, clippy::unwrap_used)] // startup
#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Bind the port early so we fail fast if another instance is running.
    let port = std::env::var("PORT").unwrap_or_else(|_| "35701".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");

    let (market_data, user_data, pool) = create_postgres_service().await;

    let auth_config = AuthConfig::from_env();

    let market_data = Arc::new(market_data);
    let user_data = Arc::new(user_data);
    let md = market_data.market_data();

    // Wire PubSub to market data caches.
    let price_pubsub = PubSub::new(Duration::from_millis(50), 8192);
    let fx_pubsub = PubSub::new(Duration::from_millis(50), 4096);
    let entity_pubsub = PubSub::new(Duration::from_millis(100), 4096);
    md.enable_price_notifications(price_pubsub.event_sender());
    md.enable_fx_notifications(fx_pubsub.event_sender());
    price_pubsub.start();
    fx_pubsub.start();
    entity_pubsub.start();
    tracing::info!("PubSub dispatchers started");

    // Start CDC listener for live database updates.
    let _cdc = calce_data::cdc::start_cdc(
        Arc::clone(&md),
        Arc::clone(&user_data),
        entity_pubsub.event_sender(),
    );

    let sim = Arc::new(simulator::Simulator::new(Arc::clone(&md)));
    let db_sim = Arc::new(db_simulator::DbSimulator::new(
        Arc::clone(&md),
        calce_data::queries::market_data::MarketDataRepo::new(pool.clone()),
    ));

    let state = AppState {
        market_data,
        user_data,
        pool: Some(pool),
        auth_config,
        api_key_cache: ApiKeyCache::new(),
        auth_rate_limiter: rate_limit::create_auth_rate_limiter(),
        simulator: Some(sim),
        db_simulator: Some(db_sim),
        price_pubsub: Some(Arc::new(price_pubsub)),
        fx_pubsub: Some(Arc::new(fx_pubsub)),
        entity_pubsub: Some(Arc::new(entity_pubsub)),
    };

    let app = build_router(state);

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
        let market_data = Arc::new(MarketDataStore::from_memory(seed::seed_market_data()));
        let user_data = seed::seed_user_data();
        AppState {
            market_data,
            user_data: Arc::new(user_data),
            pool: None,
            auth_config: TEST_AUTH_CONFIG.clone(),
            api_key_cache: ApiKeyCache::new(),
            auth_rate_limiter: rate_limit::create_auth_rate_limiter(),
            simulator: None,
            db_simulator: None,
            price_pubsub: None,
            fx_pubsub: None,
            entity_pubsub: None,
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
