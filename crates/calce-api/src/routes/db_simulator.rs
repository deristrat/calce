use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::routing::{get, post};

use calce_data::auth::authz::require_admin;

use crate::auth::Auth;
use crate::db_simulator::{DbSimulatorConfig, DbSimulatorStats};
use crate::error::ApiError;
use crate::state::AppState;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/admin/db-simulator/start", post(start))
        .route("/v1/admin/db-simulator/stop", post(stop))
        .route("/v1/admin/db-simulator/status", get(status))
}

async fn start(
    Auth(ctx): Auth,
    State(state): State<AppState>,
    body: Option<Json<DbSimulatorConfig>>,
) -> Result<Json<DbSimulatorStats>, ApiError> {
    require_admin(&ctx)?;
    let sim = state.require_db_simulator()?;
    let cfg = body.map(|b| b.0).unwrap_or_default();
    sim.start(cfg).await;
    Ok(Json(sim.stats().await))
}

async fn stop(
    Auth(ctx): Auth,
    State(state): State<AppState>,
) -> Result<Json<DbSimulatorStats>, ApiError> {
    require_admin(&ctx)?;
    let sim = state.require_db_simulator()?;
    sim.stop().await;
    Ok(Json(sim.stats().await))
}

async fn status(
    Auth(ctx): Auth,
    State(state): State<AppState>,
) -> Result<Json<DbSimulatorStats>, ApiError> {
    require_admin(&ctx)?;
    let sim = state.require_db_simulator()?;
    Ok(Json(sim.stats().await))
}
