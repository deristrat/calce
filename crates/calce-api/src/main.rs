use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

mod auth;
mod error;
mod routes;
mod seed;
mod state;

use state::AppState;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let state = AppState {
        market_data: Arc::new(seed::seed_market_data()),
        user_data: Arc::new(seed::seed_user_data()),
    };

    let app = Router::new()
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
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("failed to bind");

    tracing::info!("Listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.expect("server error");
}
