use std::convert::Infallible;

use axum::Router;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, Sse};
use axum::routing::get;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;

use crate::auth::require_admin_from_sse;
use crate::error::ApiError;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/v1/events", get(events_sse))
}

#[derive(Serialize)]
struct EntityUpdate {
    #[serde(rename = "type")]
    update_type: &'static str,
    table: String,
    id: String,
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

    let entity_pubsub = state
        .entity_pubsub
        .as_ref()
        .ok_or_else(|| ApiError::BadRequest("entity pubsub not available".into()))?;

    let sub = entity_pubsub.subscribe_all(256);

    let stream = tokio_stream::wrappers::ReceiverStream::new(sub.receiver).map(|event| {
        let key_str = match &event {
            calce_datastructs::pubsub::UpdateEvent::CurrentChanged { key } => key.as_str(),
            calce_datastructs::pubsub::UpdateEvent::HistoryChanged { key } => key.as_str(),
        };
        let (table, id) = key_str.split_once(':').unwrap_or(("unknown", key_str));
        let update = EntityUpdate {
            update_type: "entity",
            table: table.to_owned(),
            id: id.to_owned(),
        };
        let data = serde_json::to_string(&update).unwrap_or_default();
        Ok::<_, Infallible>(Event::default().event("update").data(data))
    });

    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new().interval(std::time::Duration::from_secs(15)),
    ))
}
