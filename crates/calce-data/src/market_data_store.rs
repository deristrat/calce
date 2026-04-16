use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;

use crate::concurrent_market_data::{ConcurrentMarketData, MarketDataBuilder};

pub struct MarketDataStore {
    market_data: Arc<ConcurrentMarketData>,
    instruments: HashMap<i64, InstrumentSummary>,
}

#[derive(Clone, Serialize)]
pub struct FxRateSummary {
    pub from_currency: String,
    pub to_currency: String,
    pub pair: String,
    pub data_points: usize,
    pub latest_rate: Option<f64>,
}

#[derive(Clone, Serialize)]
pub struct InstrumentSummary {
    pub id: i64,
    pub ticker: String,
    pub currency: String,
    pub name: Option<String>,
    pub instrument_type: String,
    #[serde(default)]
    pub allocations: HashMap<String, Vec<(String, f64)>>,
}

impl MarketDataStore {
    pub fn from_memory(md: MarketDataBuilder) -> Self {
        let concurrent = ConcurrentMarketData::from_builder(md);
        let instruments: HashMap<i64, InstrumentSummary> = concurrent
            .instrument_ids()
            .into_iter()
            .enumerate()
            .map(|(i, id)| {
                let db_id = i64::try_from(i + 1).unwrap_or(0);
                let summary = InstrumentSummary {
                    id: db_id,
                    ticker: id.as_str().to_owned(),
                    currency: String::new(),
                    name: None,
                    instrument_type: "other".to_owned(),
                    allocations: HashMap::new(),
                };
                (db_id, summary)
            })
            .collect();

        Self {
            market_data: Arc::new(concurrent),
            instruments,
        }
    }

    pub(crate) fn from_parts(
        md: ConcurrentMarketData,
        instruments: Vec<InstrumentSummary>,
    ) -> Self {
        let instruments = instruments.into_iter().map(|i| (i.id, i)).collect();
        Self {
            market_data: Arc::new(md),
            instruments,
        }
    }

    /// Consume the store and return the instrument list.
    pub fn into_instruments(self) -> Vec<InstrumentSummary> {
        self.instruments.into_values().collect()
    }

    pub fn market_data(&self) -> Arc<ConcurrentMarketData> {
        Arc::clone(&self.market_data)
    }

    pub fn list_instruments(&self) -> Vec<InstrumentSummary> {
        self.instruments.values().cloned().collect()
    }

    pub fn get_instrument(&self, id: i64) -> Option<InstrumentSummary> {
        self.instruments.get(&id).cloned()
    }

    pub fn instrument_count(&self) -> i64 {
        i64::try_from(self.instruments.len()).unwrap_or(0)
    }

    pub fn price_count(&self) -> i64 {
        i64::try_from(self.market_data.price_count()).unwrap_or(0)
    }

    pub fn fx_rate_count(&self) -> i64 {
        i64::try_from(self.market_data.fx_rate_count()).unwrap_or(0)
    }

    pub fn list_fx_rates(&self) -> Vec<FxRateSummary> {
        self.market_data
            .fx_rate_pairs()
            .into_iter()
            .map(|(from, to, data_points, latest_rate)| FxRateSummary {
                from_currency: from.as_str().to_owned(),
                to_currency: to.as_str().to_owned(),
                pair: format!("{}/{}", from.as_str(), to.as_str()),
                data_points,
                latest_rate,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calce_core::domain::currency::Currency;
    use calce_core::domain::fx_rate::FxRate;
    use calce_core::domain::instrument::InstrumentId;
    use calce_core::domain::price::Price;
    use calce_core::services::market_data::MarketDataService;
    use chrono::NaiveDate;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
    }

    #[test]
    fn market_data_returns_shared_arc() {
        let usd = Currency::new("USD");
        let sek = Currency::new("SEK");
        let aapl = InstrumentId::new("AAPL");

        let mut md = MarketDataBuilder::new();
        md.add_price(&aapl, date(2025, 1, 10), Price::new(150.0));
        md.add_fx_rate(FxRate::new(usd, sek, 10.5), date(2025, 1, 10));

        let store = MarketDataStore::from_memory(md);
        let arc = store.market_data();
        let price = arc.get_price(&aapl, date(2025, 1, 10)).unwrap();
        assert_eq!(price.value(), 150.0);
    }

    #[test]
    fn get_instrument_is_direct_lookup() {
        let md = MarketDataBuilder::new();
        let instruments = vec![InstrumentSummary {
            id: 42,
            ticker: "AAPL".to_owned(),
            currency: "USD".to_owned(),
            name: Some("Apple Inc.".to_owned()),
            instrument_type: "stock".to_owned(),
            allocations: HashMap::new(),
        }];
        let concurrent = ConcurrentMarketData::from_builder(md);
        let store = MarketDataStore::from_parts(concurrent, instruments);
        assert_eq!(store.get_instrument(42).unwrap().ticker, "AAPL");
        assert!(store.get_instrument(999).is_none());
    }
}
