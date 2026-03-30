//! CDC integration: starts a background listener that applies database changes
//! to the in-memory caches.

use std::sync::Arc;

use tokio::task::JoinHandle;

use crate::ConcurrentMarketData;

/// Start the CDC listener if enabled via `CALCE_CDC_ENABLED` (default: true).
///
/// Spawns two background tasks:
/// 1. The [`calce_cdc::CdcListener`] that streams WAL changes from Postgres.
/// 2. An event consumer that applies [`calce_cdc::CdcEvent`]s to the cache via
///    `set_current_price` / `set_current_fx_rate`.
///
/// Returns `None` if CDC is disabled or `DATABASE_URL` is not set.
#[must_use]
pub fn start_cdc(md: Arc<ConcurrentMarketData>) -> Option<JoinHandle<()>> {
    let config = calce_cdc::CdcConfig::from_env()?;

    let (listener, mut rx) = calce_cdc::CdcListener::new(config, 4096);

    // Consumer task: apply events to the cache
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
