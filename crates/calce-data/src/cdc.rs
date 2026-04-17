//! CDC integration: starts a background listener that applies database changes
//! to the in-memory caches and notifies entity subscribers.

use std::collections::HashMap;
use std::sync::Arc;

use calce_cdc::{CdcEvent, CdcOperation};
use calce_core::domain::currency::Currency;
use calce_core::domain::instrument::InstrumentId;
use calce_datastructs::pubsub::UpdateEvent;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::ConcurrentMarketData;
use crate::user_data_store::UserDataStore;

/// Start the CDC listener if enabled via `CALCE_CDC_ENABLED` (default: true).
///
/// Spawns two background tasks:
/// 1. The [`calce_cdc::CdcListener`] that streams WAL changes from Postgres.
/// 2. An event consumer that decodes events, applies market-data changes, and
///    forwards row changes to the `entity_tx` channel.
///
/// `instrument_seed` bootstraps the `id → ticker` map used to decode `prices`
/// rows; the consumer keeps it fresh from replicated `instruments` rows.
///
/// Returns `None` if CDC is disabled or `DATABASE_URL` is not set.
#[must_use]
pub fn start_cdc<I>(
    md: Arc<ConcurrentMarketData>,
    user_data: Arc<UserDataStore>,
    instrument_seed: I,
    entity_tx: mpsc::Sender<UpdateEvent<String>>,
) -> Option<JoinHandle<()>>
where
    I: IntoIterator<Item = (i64, String)>,
{
    let config = calce_cdc::CdcConfig::from_env()?;
    let (listener, mut rx) = calce_cdc::CdcListener::new(config, 4096);

    let mut instruments: HashMap<i64, String> = instrument_seed.into_iter().collect();
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            dispatch(&event, &md, &user_data, &entity_tx, &mut instruments).await;
        }
    });

    let handle = tokio::spawn(async move {
        listener.run().await;
    });

    tracing::info!("CDC listener started");
    Some(handle)
}

async fn dispatch(
    event: &CdcEvent,
    md: &ConcurrentMarketData,
    user_data: &UserDataStore,
    entity_tx: &mpsc::Sender<UpdateEvent<String>>,
    instruments: &mut HashMap<i64, String>,
) {
    if event.table == "instruments" && event.operation != CdcOperation::Delete {
        update_instrument_map(&event.columns, instruments);
    }

    match event.table.as_str() {
        "prices" if event.operation != CdcOperation::Delete => {
            apply_price(&event.columns, instruments, md);
        }
        "fx_rates" if event.operation != CdcOperation::Delete => {
            apply_fx_rate(&event.columns, md);
        }
        _ => {
            if event.table == "users" {
                apply_user(&event.columns, user_data);
            }
            let entity_id = event
                .columns
                .get("external_id")
                .or_else(|| event.columns.get("id"))
                .and_then(|v| v.as_deref())
                .unwrap_or("unknown");
            let key = format!("{}:{}", event.table, entity_id);
            let _ = entity_tx.send(UpdateEvent::CurrentChanged { key }).await;
        }
    }
}

fn update_instrument_map(
    columns: &HashMap<String, Option<String>>,
    instruments: &mut HashMap<i64, String>,
) {
    let Some(Some(id_str)) = columns.get("id") else {
        return;
    };
    let Some(Some(ticker)) = columns.get("ticker") else {
        return;
    };
    if let Ok(id) = id_str.parse::<i64>() {
        instruments.insert(id, ticker.clone());
    }
}

fn apply_price(
    columns: &HashMap<String, Option<String>>,
    instruments: &HashMap<i64, String>,
    md: &ConcurrentMarketData,
) {
    let Some(db_id) = col_i64(columns, "instrument_id") else {
        return;
    };
    let Some(ticker) = instruments.get(&db_id) else {
        tracing::debug!("CDC price for unknown instrument_id={db_id}, skipping");
        return;
    };
    let Some(price) = col_f64(columns, "price") else {
        return;
    };
    if let Err(e) = md.set_current_price(&InstrumentId::new(ticker), price) {
        tracing::warn!("CDC price update failed for {ticker}: {e}");
    }
}

fn apply_fx_rate(columns: &HashMap<String, Option<String>>, md: &ConcurrentMarketData) {
    let Some(from) = col_str(columns, "from_currency") else {
        return;
    };
    let Some(to) = col_str(columns, "to_currency") else {
        return;
    };
    let Some(rate) = col_f64(columns, "rate") else {
        return;
    };
    let from_currency = Currency::new(from.trim());
    let to_currency = Currency::new(to.trim());
    if let Err(e) = md.set_current_fx_rate(from_currency, to_currency, rate) {
        tracing::warn!("CDC FX update failed for {from_currency}/{to_currency}: {e}");
    }
}

fn apply_user(columns: &HashMap<String, Option<String>>, user_data: &UserDataStore) {
    let Some(uid) = columns.get("external_id").and_then(|v| v.as_deref()) else {
        return;
    };
    let name = columns.get("name").and_then(|v| v.as_deref());
    let email = columns.get("email").and_then(|v| v.as_deref());
    user_data.update_user_info(uid, name, email);
}

fn col_str<'a>(columns: &'a HashMap<String, Option<String>>, key: &str) -> Option<&'a str> {
    columns.get(key).and_then(|v| v.as_deref())
}

fn col_i64(columns: &HashMap<String, Option<String>>, key: &str) -> Option<i64> {
    col_str(columns, key)?.parse().ok()
}

fn col_f64(columns: &HashMap<String, Option<String>>, key: &str) -> Option<f64> {
    col_str(columns, key)?.parse().ok()
}
