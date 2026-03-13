use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;

use calce_core::domain::currency::Currency;
use calce_core::domain::instrument::InstrumentId;
use calce_core::domain::price::Price;
use calce_core::domain::trade::Trade;
use calce_core::domain::user::UserId;
use calce_core::services::market_data::{InMemoryMarketDataService, MarketDataService};

use calce_data::backend::{DataBackend, DataStats, DateRange, InstrumentSummary, UserSummary};
use calce_data::error::{DataError, DataResult};

/// Read-only market data backend backed by a pre-loaded Njorda cache.
pub struct NjordaBackend {
    market_data: Arc<InMemoryMarketDataService>,
}

impl NjordaBackend {
    pub fn new(market_data: InMemoryMarketDataService) -> Self {
        Self {
            market_data: Arc::new(market_data),
        }
    }
}

#[async_trait]
impl DataBackend for NjordaBackend {
    async fn load_trades(&self, user_id: &UserId) -> DataResult<Vec<Trade>> {
        Err(DataError::NoTradesFound(user_id.clone()))
    }

    async fn load_market_data(
        &self,
        _instruments: &[InstrumentId],
        _currencies: &[Currency],
        _base_currency: Currency,
        _date_range: &DateRange,
    ) -> DataResult<Arc<InMemoryMarketDataService>> {
        Ok(Arc::clone(&self.market_data))
    }

    async fn list_users(&self) -> DataResult<Vec<UserSummary>> {
        Ok(vec![])
    }

    async fn list_instruments(&self) -> DataResult<Vec<InstrumentSummary>> {
        Ok(self
            .market_data
            .instrument_ids()
            .into_iter()
            .map(|id| InstrumentSummary {
                id: id.as_str().to_owned(),
                currency: String::new(),
                name: None,
            })
            .collect())
    }

    async fn data_stats(&self) -> DataResult<DataStats> {
        Ok(DataStats {
            user_count: 0,
            instrument_count: i64::try_from(self.market_data.instrument_count()).unwrap_or(0),
            trade_count: 0,
            price_count: i64::try_from(self.market_data.price_count()).unwrap_or(0),
            fx_rate_count: i64::try_from(self.market_data.fx_rate_count()).unwrap_or(0),
        })
    }

    async fn price_history(
        &self,
        instrument: &InstrumentId,
        from: NaiveDate,
        to: NaiveDate,
    ) -> DataResult<Vec<(NaiveDate, f64)>> {
        let history: Vec<(NaiveDate, Price)> =
            self.market_data.get_price_history(instrument, from, to)?;
        Ok(history.into_iter().map(|(d, p)| (d, p.value())).collect())
    }

    fn cached_market_data(&self) -> Option<&dyn MarketDataService> {
        Some(&*self.market_data)
    }
}
