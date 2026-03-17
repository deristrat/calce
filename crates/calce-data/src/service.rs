use std::sync::Arc;

use serde::Serialize;

use crate::auth::SecurityContext;
use crate::error::{DataError, DataResult};
use crate::permissions;
use crate::queries::market_data::MarketDataRepo;
use crate::queries::user_data::UserDataRepo;

use calce_core::domain::currency::Currency;
use calce_core::domain::instrument::InstrumentId;
use calce_core::domain::price::Price;
use calce_core::domain::user::UserId;
use calce_core::inputs::CalcInputs;
use calce_core::services::market_data::{InMemoryMarketDataService, MarketDataService};
use calce_core::services::user_data::InMemoryUserDataService;
use chrono::NaiveDate;
use sqlx::PgPool;

pub struct DateRange {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

pub struct CalcInputSpec {
    pub subjects: Vec<UserId>,
    pub base_currency: Currency,
    pub date_range: DateRange,
}

pub struct DataService {
    market_data: Arc<InMemoryMarketDataService>,
    user_data: InMemoryUserDataService,
    users: Vec<UserSummary>,
    instruments: Vec<InstrumentSummary>,
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

impl DataService {
    /// Bulk-load all data from Postgres into memory at startup.
    ///
    /// # Errors
    ///
    /// Propagates database errors.
    pub async fn from_postgres(pool: &PgPool) -> DataResult<Self> {
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
            .map(|(id, email, trade_count)| UserSummary {
                id,
                email,
                trade_count,
            })
            .collect();

        let instruments: Vec<InstrumentSummary> = instruments_raw
            .into_iter()
            .map(|(id, currency, name)| InstrumentSummary { id, currency, name })
            .collect();

        let mut md = InMemoryMarketDataService::new();
        for (instrument, date, price) in all_prices {
            md.add_price(&instrument, date, price);
        }
        for (date, rate) in all_fx_rates {
            md.add_fx_rate(rate, date);
        }
        md.freeze();

        let mut ud = InMemoryUserDataService::new();
        for trade in trades {
            ud.add_trade(trade);
        }

        tracing::info!(
            "DataService loaded: {} users, {} instruments, {} prices, {} FX rates",
            users.len(),
            instruments.len(),
            md.price_count(),
            md.fx_rate_count(),
        );

        Ok(Self {
            market_data: Arc::new(md),
            user_data: ud,
            users,
            instruments,
        })
    }

    /// Build from pre-loaded in-memory data (tests, njorda path).
    pub fn from_memory(
        market_data: InMemoryMarketDataService,
        user_data: InMemoryUserDataService,
    ) -> Self {
        let users: Vec<UserSummary> = user_data
            .user_ids()
            .into_iter()
            .map(|id| UserSummary {
                id,
                email: None,
                trade_count: 0,
            })
            .collect();

        let instruments: Vec<InstrumentSummary> = market_data
            .instrument_ids()
            .into_iter()
            .map(|id| InstrumentSummary {
                id: id.as_str().to_owned(),
                currency: String::new(),
                name: None,
            })
            .collect();

        Self {
            market_data: Arc::new(market_data),
            user_data,
            users,
            instruments,
        }
    }

    /// Load trades + all market data needed for calculations.
    ///
    /// Authorizes access to all subjects, loads their trades, and returns
    /// the shared in-memory market data service.
    ///
    /// # Errors
    ///
    /// Returns `Unauthorized` if the security context lacks access to any subject.
    /// Returns `NoTradesFound` if a subject has no trades.
    pub fn load_calc_inputs(
        &self,
        ctx: &SecurityContext,
        spec: &CalcInputSpec,
    ) -> DataResult<CalcInputs> {
        let subjects = dedup_subjects(&spec.subjects);
        let mut all_trades = Vec::new();

        for subject in &subjects {
            check_user_access(ctx, subject)?;
            let trades = self
                .user_data
                .trades_for(subject)
                .ok_or_else(|| DataError::NoTradesFound(subject.clone()))?;
            all_trades.extend(trades);
        }

        Ok(CalcInputs {
            trades: all_trades,
            market_data: Arc::clone(&self.market_data),
        })
    }

    /// List users visible to the caller.
    ///
    /// Admins see all users. Regular users see only themselves.
    pub fn list_users(&self, ctx: &SecurityContext) -> Vec<UserSummary> {
        if ctx.is_admin() {
            self.users.clone()
        } else {
            let id = ctx.user_id.as_str();
            self.users.iter().filter(|u| u.id == id).cloned().collect()
        }
    }

    pub fn list_instruments(&self) -> Vec<InstrumentSummary> {
        self.instruments.clone()
    }

    pub fn data_stats(&self) -> DataStats {
        DataStats {
            user_count: i64::try_from(self.users.len()).unwrap_or(0),
            instrument_count: i64::try_from(self.instruments.len()).unwrap_or(0),
            trade_count: i64::try_from(self.user_data.trade_count()).unwrap_or(0),
            price_count: i64::try_from(self.market_data.price_count()).unwrap_or(0),
            fx_rate_count: i64::try_from(self.market_data.fx_rate_count()).unwrap_or(0),
        }
    }

    /// Price history for a single instrument.
    ///
    /// # Errors
    ///
    /// Returns `PriceNotFound` if no prices exist in the range.
    pub fn price_history(
        &self,
        instrument: &InstrumentId,
        from: NaiveDate,
        to: NaiveDate,
    ) -> DataResult<Vec<(NaiveDate, f64)>> {
        let history: Vec<(NaiveDate, Price)> =
            self.market_data.get_price_history(instrument, from, to)?;
        Ok(history.into_iter().map(|(d, p)| (d, p.value())).collect())
    }

    pub fn load_market_data(&self) -> Arc<InMemoryMarketDataService> {
        Arc::clone(&self.market_data)
    }
}

#[derive(Clone, Serialize)]
pub struct UserSummary {
    pub id: String,
    pub email: Option<String>,
    pub trade_count: i64,
}

#[derive(Clone, Serialize)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{Role, SecurityContext};
    use calce_core::domain::account::AccountId;
    use calce_core::domain::fx_rate::FxRate;
    use calce_core::domain::price::Price;
    use calce_core::domain::quantity::Quantity;
    use calce_core::domain::trade::Trade;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
    }

    fn test_service() -> DataService {
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

        DataService::from_memory(md, ud)
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

    #[test]
    fn load_calc_inputs_enforces_access_check() {
        let svc = test_service();
        let err = svc
            .load_calc_inputs(&user_ctx("bob"), &alice_spec())
            .unwrap_err();
        assert!(matches!(err, DataError::Unauthorized { .. }));
    }

    #[test]
    fn load_calc_inputs_allows_self_access() {
        let svc = test_service();
        let inputs = svc
            .load_calc_inputs(&user_ctx("alice"), &alice_spec())
            .unwrap();
        assert_eq!(inputs.trades.len(), 1);
    }

    #[test]
    fn load_calc_inputs_allows_admin_access() {
        let svc = test_service();
        let inputs = svc.load_calc_inputs(&admin_ctx(), &alice_spec()).unwrap();
        assert_eq!(inputs.trades.len(), 1);
    }

    #[test]
    fn load_market_data_returns_shared_arc() {
        let svc = test_service();
        let md = svc.load_market_data();
        let aapl = InstrumentId::new("AAPL");
        let price = md.get_price(&aapl, date(2025, 1, 10)).unwrap();
        assert_eq!(price.value(), 150.0);
    }

    #[test]
    fn duplicate_subjects_are_deduplicated() {
        let svc = test_service();
        let spec = CalcInputSpec {
            subjects: vec![UserId::new("alice"), UserId::new("alice")],
            base_currency: Currency::new("SEK"),
            date_range: DateRange {
                from: date(2025, 1, 1),
                to: date(2025, 1, 31),
            },
        };
        let inputs = svc.load_calc_inputs(&admin_ctx(), &spec).unwrap();
        assert_eq!(inputs.trades.len(), 1);
    }

    #[test]
    fn list_users_admin_sees_all() {
        let svc = test_service();
        let users = svc.list_users(&admin_ctx());
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].id, "alice");
    }

    #[test]
    fn list_users_user_sees_only_self() {
        let svc = test_service();
        let users = svc.list_users(&user_ctx("alice"));
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].id, "alice");

        let users = svc.list_users(&user_ctx("bob"));
        assert!(users.is_empty());
    }
}
