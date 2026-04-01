//! CDC integration: starts a background listener that applies database changes
//! to the in-memory caches and notifies entity subscribers.

use std::sync::Arc;

use calce_datastructs::pubsub::UpdateEvent;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::ConcurrentMarketData;
use crate::user_data_store::UserDataStore;

/// Start the CDC listener if enabled via `CALCE_CDC_ENABLED` (default: true).
///
/// Spawns two background tasks:
/// 1. The [`calce_cdc::CdcListener`] that streams WAL changes from Postgres.
/// 2. An event consumer that applies market-data events to the cache and
///    forwards entity events to the `entity_tx` channel.
///
/// Returns `None` if CDC is disabled or `DATABASE_URL` is not set.
#[must_use]
pub fn start_cdc(
    md: Arc<ConcurrentMarketData>,
    user_data: Arc<UserDataStore>,
    entity_tx: mpsc::Sender<UpdateEvent<String>>,
) -> Option<JoinHandle<()>> {
    let config = calce_cdc::CdcConfig::from_env()?;

    let (listener, mut rx) = calce_cdc::CdcListener::new(config, 4096);

    // Consumer task: route events to the appropriate destination
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                calce_cdc::CdcEvent::PriceChanged {
                    instrument_id,
                    price,
                    ..
                } => {
                    if let Err(e) = md.set_current_price(&instrument_id, price) {
                        tracing::warn!("CDC price update failed for {instrument_id}: {e}");
                    }
                }
                calce_cdc::CdcEvent::FxRateChanged {
                    from_currency,
                    to_currency,
                    rate,
                    ..
                } => {
                    if let Err(e) = md.set_current_fx_rate(from_currency, to_currency, rate) {
                        tracing::warn!(
                            "CDC FX update failed for {from_currency}/{to_currency}: {e}"
                        );
                    }
                }
                calce_cdc::CdcEvent::EntityChanged { table, columns, .. } => {
                    let entity_id = columns
                        .get("external_id")
                        .or_else(|| columns.get("id"))
                        .and_then(|v| v.as_deref())
                        .unwrap_or("unknown");

                    if table == "users"
                        && let Some(uid) = columns.get("external_id").and_then(|v| v.as_deref())
                    {
                        let name = columns.get("name").and_then(|v| v.as_deref());
                        let email = columns.get("email").and_then(|v| v.as_deref());
                        user_data.update_user_info(uid, name, email);
                    }

                    let key = format!("{table}:{entity_id}");
                    let _ = entity_tx.send(UpdateEvent::CurrentChanged { key }).await;
                }
            }
        }
    });

    // Listener task
    let handle = tokio::spawn(async move {
        listener.run().await;
    });

    tracing::info!("CDC listener started");
    Some(handle)
}
