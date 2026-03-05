use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use calce_data::engine::AsyncCalcEngine;
use calce_data::repo::market_data::MarketDataRepo;
use calce_data::repo::user_data::UserDataRepo;
use sqlx::PgPool;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

mod auth;
mod error;
mod routes;
mod state;

#[cfg(test)]
mod seed;

use state::AppState;

fn build_router(state: AppState) -> Router {
    Router::new()
        .route(
            "/v1/users/{user_id}/market-value",
            get(routes::market_value),
        )
        .route(
            "/v1/users/{user_id}/portfolio",
            get(routes::portfolio_report),
        )
        .route(
            "/v1/instruments/{instrument_id}/volatility",
            get(routes::volatility),
        )
        .layer(CorsLayer::very_permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://calce:calce@localhost:5433/calce".into());

    let pool = PgPool::connect(&database_url)
        .await
        .expect("failed to connect to database");

    sqlx::migrate!("../calce-data/migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

    tracing::info!("Database connected and migrations applied");

    let engine = AsyncCalcEngine::new(
        MarketDataRepo::new(pool.clone()),
        UserDataRepo::new(pool),
    );
    let state = AppState {
        engine: Arc::new(engine),
    };

    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("failed to bind");

    tracing::info!("Listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.expect("server error");
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn test_state() -> AppState {
        let engine = AsyncCalcEngine::from_in_memory(
            seed::seed_market_data(),
            seed::seed_user_data(),
        );
        AppState {
            engine: Arc::new(engine),
        }
    }

    async fn get(uri: &str, headers: &[(&str, &str)]) -> (StatusCode, serde_json::Value) {
        let app = build_router(test_state());
        let mut req = Request::builder().uri(uri);
        for &(k, v) in headers {
            req = req.header(k, v);
        }
        let response = app
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        (status, json)
    }

    fn auth_headers() -> Vec<(&'static str, &'static str)> {
        vec![("x-user-id", "alice"), ("x-role", "admin")]
    }

    #[tokio::test]
    async fn market_value_returns_positions_and_total() {
        let (status, body) = get(
            "/v1/users/alice/market-value?as_of_date=2025-03-14&base_currency=SEK",
            &auth_headers(),
        ).await;

        assert_eq!(status, StatusCode::OK);
        assert!(body["total"]["amount"].is_number());
        assert!(body["positions"].is_array());
    }

    #[tokio::test]
    async fn market_value_missing_auth_returns_401() {
        let (status, _) = get(
            "/v1/users/alice/market-value?as_of_date=2025-03-14&base_currency=SEK",
            &[],
        ).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn market_value_bad_currency_returns_400() {
        let (status, body) = get(
            "/v1/users/alice/market-value?as_of_date=2025-03-14&base_currency=NOPE",
            &auth_headers(),
        ).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "BAD_REQUEST");
    }

    #[tokio::test]
    async fn portfolio_report_returns_mv_and_changes() {
        let (status, body) = get(
            "/v1/users/alice/portfolio?as_of_date=2025-03-14&base_currency=SEK",
            &auth_headers(),
        ).await;

        assert_eq!(status, StatusCode::OK);
        assert!(body["market_value"]["total"]["amount"].is_number());
        assert!(body["value_changes"]["daily"]["change"]["amount"].is_number());
    }

    #[tokio::test]
    async fn volatility_returns_result() {
        let (status, body) = get(
            "/v1/instruments/AAPL/volatility?as_of_date=2025-03-14&lookback_days=365",
            &auth_headers(),
        ).await;

        assert_eq!(status, StatusCode::OK);
        assert!(body["annualized_volatility"].is_number());
        assert!(body["daily_volatility"].is_number());
        assert!(body["num_observations"].is_number());
        assert!(body["start_date"].is_string());
        assert!(body["end_date"].is_string());
    }

    #[tokio::test]
    async fn volatility_defaults_lookback_to_3_years() {
        let (status, body) = get(
            "/v1/instruments/AAPL/volatility?as_of_date=2025-03-14",
            &auth_headers(),
        ).await;

        assert_eq!(status, StatusCode::OK);
        assert!(body["annualized_volatility"].is_number());
    }

    #[tokio::test]
    async fn volatility_unknown_instrument_returns_error() {
        let (status, _) = get(
            "/v1/instruments/DOESNOTEXIST/volatility?as_of_date=2025-03-14",
            &auth_headers(),
        ).await;

        // PriceNotFound → 500
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn volatility_missing_auth_returns_401() {
        let (status, _) = get(
            "/v1/instruments/AAPL/volatility?as_of_date=2025-03-14&lookback_days=365",
            &[],
        ).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}
