use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::routing::{get, post};

use crate::auth::{Auth, require_admin};
use crate::db_simulator::{DbSimulator, DbSimulatorConfig, DbSimulatorStats};
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
    let sim = db_simulator(&state)?;
    let cfg = body.map(|b| b.0).unwrap_or_default();
    sim.start(cfg).await;
    Ok(Json(sim.stats().await))
}

async fn stop(
    Auth(ctx): Auth,
    State(state): State<AppState>,
) -> Result<Json<DbSimulatorStats>, ApiError> {
    require_admin(&ctx)?;
    let sim = db_simulator(&state)?;
    sim.stop().await;
    Ok(Json(sim.stats().await))
}

async fn status(
    Auth(ctx): Auth,
    State(state): State<AppState>,
) -> Result<Json<DbSimulatorStats>, ApiError> {
    require_admin(&ctx)?;
    let sim = db_simulator(&state)?;
    Ok(Json(sim.stats().await))
}

fn db_simulator(state: &AppState) -> Result<&Arc<DbSimulator>, ApiError> {
    state
        .db_simulator
        .as_ref()
        .ok_or_else(|| ApiError::BadRequest("database simulator not available".into()))
}
