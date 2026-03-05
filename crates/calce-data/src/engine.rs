use calce_core::auth::SecurityContext;
use calce_core::calc::aggregation;
use calce_core::calc::market_value::{self, MarketValueResult};
use calce_core::calc::volatility::{self, VolatilityResult};
use calce_core::context::CalculationContext;
use calce_core::domain::currency::Currency;
use calce_core::domain::fx_rate::FxRate;
use calce_core::domain::instrument::InstrumentId;
use calce_core::domain::user::UserId;
use calce_core::engine::CalcEngine;
use calce_core::error::{CalceError, CalceResult};
use calce_core::reports::portfolio::PortfolioReport;
use calce_core::services::market_data::InMemoryMarketDataService;
use calce_core::services::user_data::InMemoryUserDataService;
use chrono::NaiveDate;

use crate::repo::market_data::MarketDataRepo;
use crate::repo::user_data::UserDataRepo;

enum Backend {
    Postgres {
        market_data_repo: MarketDataRepo,
        user_data_repo: UserDataRepo,
    },
    InMemory {
        market_data: InMemoryMarketDataService,
        user_data: InMemoryUserDataService,
    },
}

/// Async orchestration layer: loads data from Postgres repos (or in-memory
/// services for tests), delegates to calce-core's sync calculation functions.
pub struct AsyncCalcEngine {
    backend: Backend,
}

impl AsyncCalcEngine {
    pub fn new(market_data_repo: MarketDataRepo, user_data_repo: UserDataRepo) -> Self {
        Self {
            backend: Backend::Postgres {
                market_data_repo,
                user_data_repo,
            },
        }
    }

    pub fn from_in_memory(
        market_data: InMemoryMarketDataService,
        user_data: InMemoryUserDataService,
    ) -> Self {
        Self {
            backend: Backend::InMemory {
                market_data,
                user_data,
            },
        }
    }

    /// # Errors
    ///
    /// Returns `Unauthorized` if the security context lacks access.
    /// Returns `NoTradesFound` if the user has no trades.
    /// Propagates price/FX lookup errors from market data.
    pub async fn market_value_for_user(
        &self,
        security_ctx: &SecurityContext,
        user_id: &UserId,
        ctx: &CalculationContext,
    ) -> CalceResult<MarketValueResult> {
        match &self.backend {
            Backend::InMemory {
                market_data,
                user_data,
            } => {
                let engine = CalcEngine::new(ctx, security_ctx, market_data, user_data);
                engine.market_value_for_user(user_id)
            }
            Backend::Postgres {
                market_data_repo,
                user_data_repo,
            } => {
                check_access(security_ctx, user_id)?;
                let trades = load_trades(user_data_repo, user_id).await?;
                let positions = aggregation::aggregate_positions(&trades, ctx.as_of_date);

                let market_data = build_market_data_for_positions(
                    market_data_repo,
                    positions.iter().map(|p| &p.instrument_id),
                    positions.iter().map(|p| p.currency),
                    ctx,
                )
                .await?;

                market_value::value_positions(&positions, ctx, &market_data)
            }
        }
    }

    /// # Errors
    ///
    /// Returns `Unauthorized` if the security context lacks access.
    /// Returns `NoTradesFound` if the user has no trades.
    /// Propagates price/FX lookup errors from market data.
    pub async fn portfolio_report_for_user(
        &self,
        security_ctx: &SecurityContext,
        user_id: &UserId,
        ctx: &CalculationContext,
    ) -> CalceResult<PortfolioReport> {
        match &self.backend {
            Backend::InMemory {
                market_data,
                user_data,
            } => {
                let engine = CalcEngine::new(ctx, security_ctx, market_data, user_data);
                engine.portfolio_report_for_user(user_id)
            }
            Backend::Postgres {
                market_data_repo,
                user_data_repo,
            } => {
                check_access(security_ctx, user_id)?;
                let trades = load_trades(user_data_repo, user_id).await?;
                let market_data =
                    build_full_market_data_for_trades(market_data_repo, &trades, ctx).await?;
                calce_core::reports::portfolio::portfolio_report(&trades, ctx, &market_data)
            }
        }
    }

    /// Instrument-scoped, no user data or auth required.
    ///
    /// # Errors
    ///
    /// Returns `InsufficientData` if the instrument lacks enough price history.
    pub async fn volatility(
        &self,
        instrument: &InstrumentId,
        as_of_date: NaiveDate,
        lookback_days: u32,
    ) -> CalceResult<VolatilityResult> {
        match &self.backend {
            Backend::InMemory { market_data, .. } => {
                volatility::calculate_volatility(
                    instrument,
                    as_of_date,
                    lookback_days,
                    market_data,
                )
            }
            Backend::Postgres {
                market_data_repo, ..
            } => {
                let from = as_of_date - chrono::Days::new(u64::from(lookback_days));
                let history = market_data_repo
                    .get_price_history(instrument, from, as_of_date)
                    .await
                    .map_err(CalceError::from)?;

                let mut svc = InMemoryMarketDataService::new();
                for (date, price) in history {
                    svc.add_price(instrument, date, price);
                }

                volatility::calculate_volatility(instrument, as_of_date, lookback_days, &svc)
            }
        }
    }
}

fn check_access(security_ctx: &SecurityContext, user_id: &UserId) -> CalceResult<()> {
    if !security_ctx.can_access(user_id) {
        return Err(CalceError::Unauthorized {
            requester: security_ctx.user_id.clone(),
            target: user_id.clone(),
        });
    }
    Ok(())
}

async fn load_trades(
    repo: &UserDataRepo,
    user_id: &UserId,
) -> CalceResult<Vec<calce_core::domain::trade::Trade>> {
    let trades = repo.get_trades(user_id).await.map_err(CalceError::from)?;
    if trades.is_empty() {
        return Err(CalceError::NoTradesFound(user_id.clone()));
    }
    Ok(trades)
}

/// Build an `InMemoryMarketDataService` with batch-loaded prices and FX rates
/// for the given positions at `ctx.as_of_date`.
async fn build_market_data_for_positions<'a>(
    repo: &MarketDataRepo,
    instruments: impl Iterator<Item = &'a InstrumentId>,
    position_currencies: impl Iterator<Item = Currency>,
    ctx: &CalculationContext,
) -> CalceResult<InMemoryMarketDataService> {
    let instrument_ids: Vec<InstrumentId> = instruments.cloned().collect();
    let currency_pairs: Vec<(Currency, Currency)> = position_currencies
        .filter(|c| *c != ctx.base_currency)
        .map(|c| (c, ctx.base_currency))
        .collect();

    let prices = repo
        .get_prices_batch(&instrument_ids, ctx.as_of_date)
        .await
        .map_err(CalceError::from)?;

    let fx_rates = repo
        .get_fx_rates_batch(&currency_pairs, ctx.as_of_date)
        .await
        .map_err(CalceError::from)?;

    let mut svc = InMemoryMarketDataService::new();
    for (id, price) in prices {
        svc.add_price(&id, ctx.as_of_date, price);
    }
    for ((_, _), rate) in fx_rates {
        svc.add_fx_rate(rate, ctx.as_of_date);
    }

    Ok(svc)
}

/// Build an `InMemoryMarketDataService` with full price history + FX rates
/// for all dates needed by `portfolio_report`.
async fn build_full_market_data_for_trades(
    repo: &MarketDataRepo,
    trades: &[calce_core::domain::trade::Trade],
    ctx: &CalculationContext,
) -> CalceResult<InMemoryMarketDataService> {
    let mut svc = InMemoryMarketDataService::new();

    let mut instruments: Vec<InstrumentId> = trades.iter().map(|t| t.instrument_id.clone()).collect();
    instruments.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    instruments.dedup_by(|a, b| a.as_str() == b.as_str());

    let mut currencies: Vec<Currency> = trades.iter().map(|t| t.currency).collect();
    currencies.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    currencies.dedup();

    // Load a generous window of price history (year-ago + buffer for value changes)
    let from = ctx.as_of_date - chrono::Days::new(400);
    for instrument in &instruments {
        let history = repo
            .get_price_history(instrument, from, ctx.as_of_date)
            .await
            .map_err(CalceError::from)?;
        for (date, price) in history {
            svc.add_price(instrument, date, price);
        }
    }

    // Load FX rates for each cross-currency pair across the date range
    let fx_pairs: Vec<(Currency, Currency)> = currencies
        .iter()
        .filter(|c| **c != ctx.base_currency)
        .map(|c| (*c, ctx.base_currency))
        .collect();

    for &(from_ccy, to_ccy) in &fx_pairs {
        let rows = sqlx::query_as::<_, (NaiveDate, f64)>(
            "SELECT rate_date, rate FROM fx_rates \
             WHERE from_currency = $1 AND to_currency = $2 \
             AND rate_date >= $3 AND rate_date <= $4 \
             ORDER BY rate_date",
        )
        .bind(from_ccy.as_str())
        .bind(to_ccy.as_str())
        .bind(from)
        .bind(ctx.as_of_date)
        .fetch_all(&repo.pool)
        .await
        .map_err(|e| CalceError::DataError(e.to_string()))?;

        for (date, rate) in rows {
            svc.add_fx_rate(FxRate::new(from_ccy, to_ccy, rate), date);
        }
    }

    Ok(svc)
}
