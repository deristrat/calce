mod in_memory;
mod postgres;

use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;

use calce_core::domain::currency::Currency;
use calce_core::domain::instrument::InstrumentId;
use calce_core::domain::trade::Trade;
use calce_core::domain::user::UserId;
use calce_core::services::market_data::{InMemoryMarketDataService, MarketDataService};

use crate::error::DataResult;

pub use in_memory::InMemoryBackend;
pub use postgres::PostgresBackend;

pub use crate::loader::{DataStats, DateRange, InstrumentSummary, UserSummary};

#[async_trait]
pub trait DataBackend: Send + Sync {
    async fn load_trades(&self, user_id: &UserId) -> DataResult<Vec<Trade>>;

    async fn load_market_data(
        &self,
        instruments: &[InstrumentId],
        currencies: &[Currency],
        base_currency: Currency,
        date_range: &DateRange,
    ) -> DataResult<Arc<InMemoryMarketDataService>>;

    async fn list_users(&self) -> DataResult<Vec<UserSummary>>;
    async fn list_instruments(&self) -> DataResult<Vec<InstrumentSummary>>;
    async fn data_stats(&self) -> DataResult<DataStats>;

    async fn price_history(
        &self,
        instrument: &InstrumentId,
        from: NaiveDate,
        to: NaiveDate,
    ) -> DataResult<Vec<(NaiveDate, f64)>>;

    /// Borrow the backing market data service directly, avoiding a clone.
    ///
    /// Backends with a pre-loaded in-memory cache (InMemory, Njorda) return
    /// `Some`. Postgres returns `None`, signalling that data must be loaded.
    fn cached_market_data(&self) -> Option<&dyn MarketDataService> {
        None
    }
}
