use serde::Serialize;

use crate::auth::SecurityContext;
use crate::backend::DataBackend;
use crate::error::{DataError, DataResult};
use crate::permissions;

use calce_core::domain::currency::Currency;
use calce_core::domain::instrument::InstrumentId;
use calce_core::domain::trade::Trade;
use calce_core::domain::user::UserId;
use calce_core::error::CalceResult;
use calce_core::inputs::CalcInputs;
use calce_core::services::market_data::MarketDataService;
use chrono::NaiveDate;

pub struct DateRange {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

pub struct CalcInputSpec {
    pub subjects: Vec<UserId>,
    pub base_currency: Currency,
    pub date_range: DateRange,
}

/// Generic async data loader.
///
/// Wraps a `DataBackend` implementation.
///
/// User-scoped methods require a `SecurityContext` and enforce access checks
/// via [`permissions::can_access_user_data`] before loading any data.
pub struct DataLoader {
    backend: Box<dyn DataBackend>,
}

fn check_user_access(security_ctx: &SecurityContext, user_id: &UserId) -> DataResult<()> {
    if !permissions::can_access_user_data(security_ctx, user_id) {
        return Err(DataError::Unauthorized {
            requester: security_ctx.user_id.clone(),
            target: user_id.clone(),
        });
    }
    Ok(())
}

impl DataLoader {
    pub fn new(backend: impl DataBackend + 'static) -> Self {
        Self {
            backend: Box::new(backend),
        }
    }

    /// Load trades + all market data needed for calculations.
    ///
    /// Authorizes access to all subjects, loads their trades, and assembles
    /// the market data needed for the combined set.
    ///
    /// # Errors
    ///
    /// Returns `Unauthorized` if the security context lacks access to any subject.
    /// Returns `NoTradesFound` if a subject has no trades.
    /// Propagates price/FX/database errors.
    pub async fn load_calc_inputs(
        &self,
        ctx: &SecurityContext,
        spec: &CalcInputSpec,
    ) -> DataResult<CalcInputs> {
        let subjects = dedup_subjects(&spec.subjects);
        let mut all_trades = Vec::new();

        for subject in &subjects {
            check_user_access(ctx, subject)?;
            let trades = self.backend.load_trades(subject).await?;
            all_trades.extend(trades);
        }

        let instruments = unique_instruments(&all_trades);
        let currencies = unique_currencies(&all_trades);

        let market_data = self
            .backend
            .load_market_data(
                &instruments,
                &currencies,
                spec.base_currency,
                &spec.date_range,
            )
            .await?;

        Ok(CalcInputs {
            trades: all_trades,
            market_data,
        })
    }

    /// List users visible to the caller.
    ///
    /// Admins see all users. Regular users see only themselves.
    ///
    /// # Errors
    ///
    /// Propagates database errors.
    pub async fn list_users(&self, ctx: &SecurityContext) -> DataResult<Vec<UserSummary>> {
        let users = self.backend.list_users().await?;
        if ctx.is_admin() {
            Ok(users)
        } else {
            let id = ctx.user_id.as_str();
            Ok(users.into_iter().filter(|u| u.id == id).collect())
        }
    }

    /// # Errors
    ///
    /// Propagates database errors.
    pub async fn list_instruments(&self) -> DataResult<Vec<InstrumentSummary>> {
        self.backend.list_instruments().await
    }

    /// # Errors
    ///
    /// Propagates database errors.
    pub async fn data_stats(&self) -> DataResult<DataStats> {
        self.backend.data_stats().await
    }

    /// Price history for a single instrument.
    ///
    /// # Errors
    ///
    /// Returns `PriceNotFound` if no prices exist in the range.
    /// Propagates database errors.
    pub async fn price_history(
        &self,
        instrument: &InstrumentId,
        from: NaiveDate,
        to: NaiveDate,
    ) -> DataResult<Vec<(NaiveDate, f64)>> {
        self.backend.price_history(instrument, from, to).await
    }

    /// Run a price-only calculation against market data.
    ///
    /// For in-memory backends (InMemory, Njorda) this borrows directly with
    /// no clone. For Postgres it loads prices for the requested instruments
    /// into a temporary service — no FX rates are loaded.
    ///
    /// # Errors
    ///
    /// Propagates price/database errors.
    pub async fn with_market_data<T>(
        &self,
        instruments: &[InstrumentId],
        date_range: &DateRange,
        f: impl FnOnce(&dyn MarketDataService) -> CalceResult<T>,
    ) -> DataResult<T> {
        // Fast path: borrow directly from cached backends (no clone)
        if let Some(md) = self.backend.cached_market_data() {
            return f(md).map_err(DataError::from);
        }
        // Postgres path: load only prices for the requested instruments
        let svc = self
            .backend
            .load_market_data(instruments, &[], Currency::new("USD"), date_range)
            .await?;
        f(&*svc).map_err(DataError::from)
    }
}

#[derive(Serialize)]
pub struct UserSummary {
    pub id: String,
    pub email: Option<String>,
    pub trade_count: i64,
}

#[derive(Serialize)]
pub struct InstrumentSummary {
    pub id: String,
    pub currency: String,
    pub name: Option<String>,
}

#[derive(Serialize)]
pub struct DataStats {
    pub user_count: i64,
    pub instrument_count: i64,
    pub trade_count: i64,
    pub price_count: i64,
    pub fx_rate_count: i64,
}

fn dedup_subjects(subjects: &[UserId]) -> Vec<UserId> {
    let mut seen = Vec::with_capacity(subjects.len());
    for s in subjects {
        if !seen.iter().any(|existing: &UserId| existing == s) {
            seen.push(s.clone());
        }
    }
    seen
}

fn unique_instruments(trades: &[Trade]) -> Vec<InstrumentId> {
    let mut instruments: Vec<InstrumentId> =
        trades.iter().map(|t| t.instrument_id.clone()).collect();
    instruments.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    instruments.dedup_by(|a, b| a.as_str() == b.as_str());
    instruments
}

fn unique_currencies(trades: &[Trade]) -> Vec<Currency> {
    let mut currencies: Vec<Currency> = trades.iter().map(|t| t.currency).collect();
    currencies.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    currencies.dedup();
    currencies
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{Role, SecurityContext};
    use calce_core::domain::account::AccountId;
    use calce_core::domain::fx_rate::FxRate;
    use calce_core::domain::price::Price;
    use calce_core::domain::quantity::Quantity;
    use calce_core::services::market_data::InMemoryMarketDataService;
    use calce_core::services::user_data::InMemoryUserDataService;

    use crate::backend::InMemoryBackend;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
    }

    fn test_loader() -> DataLoader {
        let usd = Currency::new("USD");
        let sek = Currency::new("SEK");
        let aapl = InstrumentId::new("AAPL");

        let mut md = InMemoryMarketDataService::new();
        md.add_price(&aapl, date(2025, 1, 10), Price::new(150.0));
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

        DataLoader::new(InMemoryBackend::new(md, ud))
    }

    fn admin_ctx() -> SecurityContext {
        SecurityContext {
            user_id: UserId::new("alice"),
            role: Role::Admin,
        }
    }

    fn user_ctx(user: &str) -> SecurityContext {
        SecurityContext {
            user_id: UserId::new(user),
            role: Role::User,
        }
    }

    fn alice_spec() -> CalcInputSpec {
        CalcInputSpec {
            subjects: vec![UserId::new("alice")],
            base_currency: Currency::new("SEK"),
            date_range: DateRange {
                from: date(2025, 1, 1),
                to: date(2025, 1, 31),
            },
        }
    }

    #[tokio::test]
    async fn load_calc_inputs_enforces_access_check() {
        let loader = test_loader();
        let err = loader
            .load_calc_inputs(&user_ctx("bob"), &alice_spec())
            .await
            .unwrap_err();
        assert!(matches!(err, DataError::Unauthorized { .. }));
    }

    #[tokio::test]
    async fn load_calc_inputs_allows_self_access() {
        let loader = test_loader();
        let inputs = loader
            .load_calc_inputs(&user_ctx("alice"), &alice_spec())
            .await
            .unwrap();
        assert_eq!(inputs.trades.len(), 1);
    }

    #[tokio::test]
    async fn load_calc_inputs_allows_admin_access() {
        let loader = test_loader();
        let inputs = loader
            .load_calc_inputs(&admin_ctx(), &alice_spec())
            .await
            .unwrap();
        assert_eq!(inputs.trades.len(), 1);
    }

    #[tokio::test]
    async fn with_market_data_borrows_without_cloning() {
        let loader = test_loader();
        let aapl = InstrumentId::new("AAPL");
        let date_range = DateRange {
            from: date(2025, 1, 1),
            to: date(2025, 1, 31),
        };
        let result = loader
            .with_market_data(std::slice::from_ref(&aapl), &date_range, |md| {
                let price = md.get_price(&aapl, date(2025, 1, 10))?;
                Ok(price.value())
            })
            .await
            .unwrap();
        assert_eq!(result, 150.0);
    }

    #[tokio::test]
    async fn duplicate_subjects_are_deduplicated() {
        let loader = test_loader();
        let spec = CalcInputSpec {
            subjects: vec![UserId::new("alice"), UserId::new("alice")],
            base_currency: Currency::new("SEK"),
            date_range: DateRange {
                from: date(2025, 1, 1),
                to: date(2025, 1, 31),
            },
        };
        let inputs = loader.load_calc_inputs(&admin_ctx(), &spec).await.unwrap();
        // Alice has 1 trade — duplicates in subjects must not double-count
        assert_eq!(inputs.trades.len(), 1);
    }

    #[tokio::test]
    async fn list_users_admin_sees_all() {
        let loader = test_loader();
        let users = loader.list_users(&admin_ctx()).await.unwrap();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].id, "alice");
    }

    #[tokio::test]
    async fn list_users_user_sees_only_self() {
        let loader = test_loader();
        // alice sees herself
        let users = loader.list_users(&user_ctx("alice")).await.unwrap();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].id, "alice");

        // bob sees nothing (no user record for bob)
        let users = loader.list_users(&user_ctx("bob")).await.unwrap();
        assert!(users.is_empty());
    }

    #[test]
    fn unique_instruments_deduplicates_and_sorts() {
        let usd = Currency::new("USD");
        let alice = UserId::new("alice");
        let acct = AccountId::new("a");
        let d = date(2025, 1, 1);
        let trades = vec![
            Trade {
                user_id: alice.clone(),
                account_id: acct.clone(),
                instrument_id: InstrumentId::new("MSFT"),
                quantity: Quantity::new(1.0),
                price: Price::new(1.0),
                currency: usd,
                date: d,
            },
            Trade {
                user_id: alice.clone(),
                account_id: acct.clone(),
                instrument_id: InstrumentId::new("AAPL"),
                quantity: Quantity::new(1.0),
                price: Price::new(1.0),
                currency: usd,
                date: d,
            },
            Trade {
                user_id: alice,
                account_id: acct,
                instrument_id: InstrumentId::new("MSFT"),
                quantity: Quantity::new(1.0),
                price: Price::new(1.0),
                currency: usd,
                date: d,
            },
        ];
        let result = unique_instruments(&trades);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].as_str(), "AAPL");
        assert_eq!(result[1].as_str(), "MSFT");
    }

    #[test]
    fn unique_currencies_deduplicates() {
        let usd = Currency::new("USD");
        let eur = Currency::new("EUR");
        let alice = UserId::new("alice");
        let acct = AccountId::new("a");
        let d = date(2025, 1, 1);
        let trades = vec![
            Trade {
                user_id: alice.clone(),
                account_id: acct.clone(),
                instrument_id: InstrumentId::new("A"),
                quantity: Quantity::new(1.0),
                price: Price::new(1.0),
                currency: usd,
                date: d,
            },
            Trade {
                user_id: alice.clone(),
                account_id: acct.clone(),
                instrument_id: InstrumentId::new("B"),
                quantity: Quantity::new(1.0),
                price: Price::new(1.0),
                currency: eur,
                date: d,
            },
            Trade {
                user_id: alice,
                account_id: acct,
                instrument_id: InstrumentId::new("C"),
                quantity: Quantity::new(1.0),
                price: Price::new(1.0),
                currency: usd,
                date: d,
            },
        ];
        let result = unique_currencies(&trades);
        assert_eq!(result.len(), 2);
    }
}
