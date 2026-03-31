use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;

use crate::auth::{Auth, require_admin, require_admin_from_sse};
use crate::error::ApiError;
use crate::simulator::{SimulatorConfig, SimulatorStats};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/admin/simulator/start", post(start))
        .route("/v1/admin/simulator/stop", post(stop))
        .route("/v1/admin/simulator/status", get(status))
        .route("/v1/admin/simulator/events", get(events_sse))
}

async fn start(
    Auth(ctx): Auth,
    State(state): State<AppState>,
    body: Option<Json<SimulatorConfig>>,
) -> Result<Json<SimulatorStats>, ApiError> {
    require_admin(&ctx)?;
    let sim = simulator(&state)?;
    let cfg = body.map(|b| b.0).unwrap_or_default();
    sim.start(cfg).await;
    Ok(Json(sim.stats().await))
}

async fn stop(
    Auth(ctx): Auth,
    State(state): State<AppState>,
) -> Result<Json<SimulatorStats>, ApiError> {
    require_admin(&ctx)?;
    let sim = simulator(&state)?;
    sim.stop().await;
    Ok(Json(sim.stats().await))
}

async fn status(
    Auth(ctx): Auth,
    State(state): State<AppState>,
) -> Result<Json<SimulatorStats>, ApiError> {
    require_admin(&ctx)?;
    let sim = simulator(&state)?;
    Ok(Json(sim.stats().await))
}

#[derive(Serialize)]
struct SseUpdate {
    #[serde(rename = "type")]
    update_type: &'static str,
    key: String,
    kind: &'static str,
}

#[derive(Deserialize)]
struct SseQuery {
    token: Option<String>,
}

async fn events_sse(
    headers: HeaderMap,
    Query(query): Query<SseQuery>,
    State(state): State<AppState>,
) -> Result<Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    require_admin_from_sse(&headers, query.token, &state).await?;

    let md = state.market_data.market_data();

    let price_pubsub = state
        .price_pubsub
        .as_ref()
        .ok_or_else(|| ApiError::BadRequest("pubsub not available".into()))?;
    let fx_pubsub = state
        .fx_pubsub
        .as_ref()
        .ok_or_else(|| ApiError::BadRequest("pubsub not available".into()))?;

    // Subscribe to all known keys.
    let instrument_ids = md.instrument_ids();
    let fx_keys = md.fx_pair_keys();

    let price_sub = price_pubsub.subscribe(&instrument_ids, 256);
    let fx_sub = fx_pubsub.subscribe(&fx_keys, 256);

    let price_stream =
        tokio_stream::wrappers::ReceiverStream::new(price_sub.receiver).map(|event| {
            let (key, kind) = match &event {
                calce_datastructs::pubsub::UpdateEvent::CurrentChanged { key } => {
                    (key.as_str().to_owned(), "current")
                }
                calce_datastructs::pubsub::UpdateEvent::HistoryChanged { key } => {
                    (key.as_str().to_owned(), "history")
                }
            };
            let update = SseUpdate {
                update_type: "price",
                key,
                kind,
            };
            let data = serde_json::to_string(&update).unwrap_or_default();
            Ok::<_, Infallible>(Event::default().event("update").data(data))
        });

    let fx_stream = tokio_stream::wrappers::ReceiverStream::new(fx_sub.receiver).map(|event| {
        let (key, kind) = match &event {
            calce_datastructs::pubsub::UpdateEvent::CurrentChanged { key } => {
                (format!("{}/{}", key.0.as_str(), key.1.as_str()), "current")
            }
            calce_datastructs::pubsub::UpdateEvent::HistoryChanged { key } => {
                (format!("{}/{}", key.0.as_str(), key.1.as_str()), "history")
            }
        };
        let update = SseUpdate {
            update_type: "fx",
            key,
            kind,
        };
        let data = serde_json::to_string(&update).unwrap_or_default();
        Ok::<_, Infallible>(Event::default().event("update").data(data))
    });

    let merged = price_stream.merge(fx_stream);

    Ok(Sse::new(merged).keep_alive(
        axum::response::sse::KeepAlive::new().interval(std::time::Duration::from_secs(15)),
    ))
}

fn simulator(state: &AppState) -> Result<&Arc<crate::simulator::Simulator>, ApiError> {
    state
        .simulator
        .as_ref()
        .ok_or_else(|| ApiError::BadRequest("simulator not available".into()))
}
