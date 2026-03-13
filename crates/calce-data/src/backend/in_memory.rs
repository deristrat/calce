use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;

use calce_core::domain::currency::Currency;
use calce_core::domain::instrument::InstrumentId;
use calce_core::domain::price::Price;
use calce_core::domain::trade::Trade;
use calce_core::domain::user::UserId;
use calce_core::services::market_data::{InMemoryMarketDataService, MarketDataService};
use calce_core::services::user_data::InMemoryUserDataService;

use crate::error::{DataError, DataResult};

use super::{DataBackend, DataStats, DateRange, InstrumentSummary, UserSummary};

pub struct InMemoryBackend {
    market_data: Arc<InMemoryMarketDataService>,
    user_data: InMemoryUserDataService,
}

impl InMemoryBackend {
    pub fn new(market_data: InMemoryMarketDataService, user_data: InMemoryUserDataService) -> Self {
        Self {
            market_data: Arc::new(market_data),
            user_data,
        }
    }
}

#[async_trait]
impl DataBackend for InMemoryBackend {
    async fn load_trades(&self, user_id: &UserId) -> DataResult<Vec<Trade>> {
        self.user_data
            .trades_for(user_id)
            .ok_or_else(|| DataError::NoTradesFound(user_id.clone()))
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
        // TODO this filter shoudl be pushed down
        Ok(self
            .user_data
            .user_ids()
            .into_iter()
            .map(|id| UserSummary {
                id,
                email: None,
                trade_count: 0,
            })
            .collect())
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
            user_count: i64::try_from(self.user_data.user_count()).unwrap_or(0),
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

#[cfg(test)]
mod tests {
    use super::*;
    use calce_core::domain::account::AccountId;
    use calce_core::domain::fx_rate::FxRate;
    use calce_core::domain::quantity::Quantity;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
    }

    fn test_backend() -> InMemoryBackend {
        let usd = Currency::new("USD");
        let sek = Currency::new("SEK");
        let aapl = InstrumentId::new("AAPL");

        let mut md = InMemoryMarketDataService::new();
        md.add_price(&aapl, date(2025, 1, 10), Price::new(150.0));
        md.add_price(&aapl, date(2025, 1, 11), Price::new(152.0));
        md.add_fx_rate(FxRate::new(usd, sek, 10.5), date(2025, 1, 10));
        md.freeze();

        let mut ud = InMemoryUserDataService::new();
        ud.add_trade(Trade {
            user_id: UserId::new("alice"),
            account_id: AccountId::new("alice-usd"),
            instrument_id: aapl,
            quantity: Quantity::new(100.0),
            price: Price::new(150.0),
            currency: usd,
            date: date(2025, 1, 10),
        });

        InMemoryBackend::new(md, ud)
    }

    #[tokio::test]
    async fn load_trades_returns_trades_for_known_user() {
        let backend = test_backend();
        let trades = backend.load_trades(&UserId::new("alice")).await.unwrap();
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].instrument_id, InstrumentId::new("AAPL"));
    }

    #[tokio::test]
    async fn load_trades_returns_not_found_for_unknown_user() {
        let backend = test_backend();
        let err = backend.load_trades(&UserId::new("bob")).await.unwrap_err();
        assert!(matches!(err, DataError::NoTradesFound(_)));
    }

    #[tokio::test]
    async fn load_market_data_returns_shared_arc() {
        let backend = test_backend();
        let range = DateRange {
            from: date(2025, 1, 1),
            to: date(2025, 1, 31),
        };
        let md = backend
            .load_market_data(&[], &[], Currency::new("USD"), &range)
            .await
            .unwrap();
        // Should be the same Arc — no deep clone
        assert!(Arc::ptr_eq(&md, &backend.market_data));
    }

    #[tokio::test]
    async fn list_users_returns_known_users() {
        let backend = test_backend();
        let users = backend.list_users().await.unwrap();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].id, "alice");
    }

    #[tokio::test]
    async fn list_instruments_returns_ids() {
        let backend = test_backend();
        let instruments = backend.list_instruments().await.unwrap();
        assert_eq!(instruments.len(), 1);
        assert_eq!(instruments[0].id, "AAPL");
    }

    #[tokio::test]
    async fn data_stats_returns_counts() {
        let backend = test_backend();
        let stats = backend.data_stats().await.unwrap();
        assert_eq!(stats.user_count, 1);
        assert_eq!(stats.instrument_count, 1);
        assert_eq!(stats.price_count, 2);
        assert_eq!(stats.fx_rate_count, 1);
        assert_eq!(stats.trade_count, 0); // in-memory doesn't track trade count
    }

    #[tokio::test]
    async fn price_history_returns_prices() {
        let backend = test_backend();
        let history = backend
            .price_history(
                &InstrumentId::new("AAPL"),
                date(2025, 1, 1),
                date(2025, 1, 31),
            )
            .await
            .unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].1, 150.0);
        assert_eq!(history[1].1, 152.0);
    }

    #[tokio::test]
    async fn cached_market_data_returns_some() {
        let backend = test_backend();
        assert!(backend.cached_market_data().is_some());
    }
}
