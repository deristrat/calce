use std::collections::HashMap;

use calce_core::domain::instrument::{InstrumentId, InstrumentType};

use crate::error::DataResult;
use crate::in_memory_market_data::InMemoryMarketDataService;
use crate::market_data_store::{InstrumentSummary, MarketDataStore};
use crate::queries::market_data::MarketDataRepo;
use crate::queries::user_data::UserDataRepo;
use crate::user_data_store::{UserDataStore, UserSummary};
use sqlx::PgPool;

/// Bulk-load all data from Postgres into memory at startup.
///
/// # Errors
///
/// Propagates database errors.
pub async fn load_from_postgres(pool: &PgPool) -> DataResult<(MarketDataStore, UserDataStore)> {
    let md_repo = MarketDataRepo::new(pool.clone());
    let ud_repo = UserDataRepo::new(pool.clone());

    let (users_raw, instruments_raw, trades, all_prices, all_fx_rates) = tokio::try_join!(
        ud_repo.list_users_with_trade_counts(),
        md_repo.list_instruments(),
        ud_repo.get_all_trades(),
        md_repo.get_all_prices(),
        md_repo.get_all_fx_rates(),
    )?;

    let users: Vec<UserSummary> = users_raw
        .into_iter()
        .map(|row| UserSummary {
            id: row.external_id,
            email: row.email,
            name: row.name,
            organization_id: row.organization_id,
            organization_name: row.organization_name,
            trade_count: row.trade_count.unwrap_or(0),
            account_count: row.account_count.unwrap_or(0),
        })
        .collect();

    let instruments: Vec<InstrumentSummary> = instruments_raw
        .into_iter()
        .map(
            |(id, ticker, currency, name, instrument_type, alloc_json)| {
                let allocations: HashMap<String, Vec<(String, f64)>> =
                    parse_allocations_json(&alloc_json);
                InstrumentSummary {
                    id,
                    ticker,
                    currency,
                    name,
                    instrument_type,
                    allocations,
                }
            },
        )
        .collect();

    let mut md = InMemoryMarketDataService::new();
    for (instrument, date, price) in all_prices {
        md.add_price(&instrument, date, price);
    }
    for (date, rate) in all_fx_rates {
        md.add_fx_rate(rate, date);
    }
    for instr in &instruments {
        let iid = InstrumentId::new(&instr.ticker);
        md.add_instrument_type(&iid, InstrumentType::from_str_lossy(&instr.instrument_type));
        for (dimension, weights) in &instr.allocations {
            for (key, weight) in weights {
                md.add_allocation(&iid, dimension, key, *weight);
            }
        }
    }
    md.freeze();

    let mut ud = UserDataStore::new();
    for trade in trades {
        ud.add_trade(trade);
    }
    ud.set_users(users);

    tracing::info!(
        "Data loaded: {} users, {} instruments, {} prices, {} FX rates",
        ud.user_count(),
        instruments.len(),
        md.price_count(),
        md.fx_rate_count(),
    );

    let market_store = MarketDataStore::from_parts(md, instruments);

    Ok((market_store, ud))
}

/// Parse the JSONB allocations column into a dimension → weights map.
///
/// Expected shape: `{"sector": {"Information Technology": 0.3, "Health Care": 0.13}, ...}`
fn parse_allocations_json(value: &serde_json::Value) -> HashMap<String, Vec<(String, f64)>> {
    let mut result = HashMap::new();
    if let serde_json::Value::Object(dimensions) = value {
        for (dim, keys_val) in dimensions {
            if let serde_json::Value::Object(keys) = keys_val {
                let weights: Vec<(String, f64)> = keys
                    .iter()
                    .filter_map(|(k, v)| v.as_f64().map(|w| (k.clone(), w)))
                    .collect();
                if !weights.is_empty() {
                    result.insert(dim.clone(), weights);
                }
            }
        }
    }
    result
}
