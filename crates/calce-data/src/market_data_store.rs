use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;

use crate::in_memory_market_data::InMemoryMarketDataService;

pub struct MarketDataStore {
    market_data: Arc<InMemoryMarketDataService>,
    instruments: Vec<InstrumentSummary>,
}

#[derive(Clone, Serialize)]
pub struct InstrumentSummary {
    pub id: String,
    pub currency: String,
    pub name: Option<String>,
    pub instrument_type: String,
    #[serde(default)]
    pub allocations: HashMap<String, Vec<(String, f64)>>,
}

impl MarketDataStore {
    pub fn from_memory(md: InMemoryMarketDataService) -> Self {
        let instruments: Vec<InstrumentSummary> = md
            .instrument_ids()
            .into_iter()
            .map(|id| InstrumentSummary {
                id: id.as_str().to_owned(),
                currency: String::new(),
                name: None,
                instrument_type: "other".to_owned(),
                allocations: HashMap::new(),
            })
            .collect();

        Self {
            market_data: Arc::new(md),
            instruments,
        }
    }

    pub(crate) fn from_parts(
        md: InMemoryMarketDataService,
        instruments: Vec<InstrumentSummary>,
    ) -> Self {
        Self {
            market_data: Arc::new(md),
            instruments,
        }
    }

    /// Consume the store and return the inner market data service and instrument list.
    pub fn into_parts(self) -> (InMemoryMarketDataService, Vec<InstrumentSummary>) {
        let md = Arc::try_unwrap(self.market_data)
            .unwrap_or_else(|arc| (*arc).clone());
        (md, self.instruments)
    }

    pub fn market_data(&self) -> Arc<InMemoryMarketDataService> {
        Arc::clone(&self.market_data)
    }

    pub fn list_instruments(&self) -> Vec<InstrumentSummary> {
        self.instruments.clone()
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

        let mut md = InMemoryMarketDataService::new();
        md.add_price(&aapl, date(2025, 1, 10), Price::new(150.0));
        md.add_fx_rate(FxRate::new(usd, sek, 10.5), date(2025, 1, 10));
        md.freeze();

        let store = MarketDataStore::from_memory(md);
        let arc = store.market_data();
        let price = arc.get_price(&aapl, date(2025, 1, 10)).unwrap();
        assert_eq!(price.value(), 150.0);
    }
}
