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

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/events", get(entity_events_sse))
        .route("/v1/admin/market-events", get(market_events_sse))
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

async fn entity_events_sse(
    headers: HeaderMap,
    Query(query): Query<SseQuery>,
    State(state): State<AppState>,
) -> Result<Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    require_admin_from_sse(&headers, query.token, &state).await?;

    let entity_pubsub = state.require_entity_pubsub()?;

    let sub = entity_pubsub.subscribe_all(256);

    let stream = tokio_stream::wrappers::ReceiverStream::new(sub.receiver).map(|event| {
        let key_str = match &event {
            calce_datastructs::pubsub::UpdateEvent::CurrentChanged { key }
            | calce_datastructs::pubsub::UpdateEvent::HistoryChanged { key } => key.as_str(),
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

#[derive(Serialize)]
struct MarketUpdate {
    #[serde(rename = "type")]
    update_type: &'static str,
    key: String,
    kind: &'static str,
}

/// Live stream of price + FX cache changes. Fed by the CDC listener (and the
/// DB simulator when running). Used by the admin console to visualize the
/// replication pipeline.
async fn market_events_sse(
    headers: HeaderMap,
    Query(query): Query<SseQuery>,
    State(state): State<AppState>,
) -> Result<Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    require_admin_from_sse(&headers, query.token, &state).await?;

    let md = state.market_data.market_data();

    let price_pubsub = state.require_price_pubsub()?;
    let fx_pubsub = state.require_fx_pubsub()?;

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
            let update = MarketUpdate {
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
        let update = MarketUpdate {
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
