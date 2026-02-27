use chrono::{Datelike, NaiveDate};

use crate::context::CalculationContext;
use crate::domain::money::Money;
use crate::domain::trade::Trade;
use crate::error::CalceResult;
use crate::services::market_data::MarketDataService;

use super::aggregation::aggregate_positions;
use super::market_value::{value_positions, MarketValueResult};

#[derive(Debug)]
pub struct ValueChange {
    pub current: Money,
    pub previous: Money,
    pub change: Money,
    /// Percentage change (0.05 = 5%). `None` when previous value is zero.
    pub change_pct: Option<f64>,
}

#[derive(Debug)]
pub struct ValueChangeSummary {
    pub market_value: Money,
    pub daily: ValueChange,
    pub weekly: ValueChange,
    pub yearly: ValueChange,
    pub ytd: ValueChange,
}

/// `#CALC_VCHG`
///
/// Compute the change between two market value snapshots.
///
/// Both results must be in the same base currency (enforced by
/// `CalculationContext`).
///
/// # Errors
///
/// Returns `CurrencyMismatch` if the totals are in different currencies.
pub fn value_change(
    current: &MarketValueResult,
    previous: &MarketValueResult,
) -> CalceResult<ValueChange> {
    let change = current.total.checked_add(Money::new(-previous.total.amount, previous.total.currency))?;
    let change_pct = if previous.total.amount == 0.0 {
        None
    } else {
        Some(change.amount / previous.total.amount)
    };
    Ok(ValueChange {
        current: current.total,
        previous: previous.total,
        change,
        change_pct,
    })
}

/// `#CALC_VCHG` — summary across standard periods.
///
/// Calls `aggregate_positions` and `value_positions` at each comparison date,
/// then computes the change for each period.
///
/// # Errors
///
/// Returns errors from `value_positions` (missing prices or FX rates).
pub fn value_change_summary(
    trades: &[Trade],
    ctx: &CalculationContext,
    market_data: &dyn MarketDataService,
) -> CalceResult<ValueChangeSummary> {
    let current = market_value_at(trades, ctx.as_of_date, ctx, market_data)?;
    value_change_summary_from(&current, trades, ctx, market_data)
}

/// `#CALC_VCHG` — summary using a pre-computed current-date market value.
///
/// Same as `value_change_summary` but skips recomputing the current snapshot,
/// useful when the caller already has the current `MarketValueResult`
/// (e.g. from a portfolio report that bundles multiple calculations).
///
/// # Errors
///
/// Returns errors from `value_positions` (missing prices or FX rates).
pub fn value_change_summary_from(
    current: &MarketValueResult,
    trades: &[Trade],
    ctx: &CalculationContext,
    market_data: &dyn MarketDataService,
) -> CalceResult<ValueChangeSummary> {
    let day_ago = ctx.as_of_date - chrono::Days::new(1);
    let week_ago = ctx.as_of_date - chrono::Days::new(7);
    let year_ago = prev_year(ctx.as_of_date);
    // Dec 31 of previous year
    let ytd_start = NaiveDate::from_ymd_opt(ctx.as_of_date.year() - 1, 12, 31)
        .unwrap_or(ctx.as_of_date);

    let daily = value_change(current, &market_value_at(trades, day_ago, ctx, market_data)?)?;
    let weekly = value_change(current, &market_value_at(trades, week_ago, ctx, market_data)?)?;
    let yearly = value_change(current, &market_value_at(trades, year_ago, ctx, market_data)?)?;
    let ytd = value_change(current, &market_value_at(trades, ytd_start, ctx, market_data)?)?;

    Ok(ValueChangeSummary {
        market_value: current.total,
        daily,
        weekly,
        yearly,
        ytd,
    })
}

fn market_value_at(
    trades: &[Trade],
    date: NaiveDate,
    ctx: &CalculationContext,
    market_data: &dyn MarketDataService,
) -> CalceResult<MarketValueResult> {
    let positions = aggregate_positions(trades, date);
    let point_ctx = CalculationContext::new(ctx.base_currency, date);
    value_positions(&positions, &point_ctx, market_data)
}

/// Same date one year earlier, handling leap years (Feb 29 → Feb 28).
fn prev_year(date: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(date.year() - 1, date.month(), date.day())
        .or_else(|| NaiveDate::from_ymd_opt(date.year() - 1, date.month(), date.day() - 1))
        .unwrap_or(date)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::account::AccountId;
    use crate::domain::currency::Currency;
    use crate::domain::fx_rate::FxRate;
    use crate::domain::instrument::InstrumentId;
    use crate::domain::price::Price;
    use crate::domain::quantity::Quantity;
    use crate::domain::user::UserId;
    use crate::services::market_data::InMemoryMarketDataService;

    #[test]
    fn value_change_computes_diff() {
        let sek = Currency::new("SEK");
        let current = MarketValueResult {
            positions: vec![],
            total: Money::new(110_000.0, sek),
        };
        let previous = MarketValueResult {
            positions: vec![],
            total: Money::new(100_000.0, sek),
        };

        let change = value_change(&current, &previous).unwrap();
        assert_eq!(change.change.amount, 10_000.0);
        assert!((change.change_pct.unwrap() - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn value_change_from_zero_has_no_pct() {
        let sek = Currency::new("SEK");
        let current = MarketValueResult {
            positions: vec![],
            total: Money::new(50_000.0, sek),
        };
        let previous = MarketValueResult {
            positions: vec![],
            total: Money::new(0.0, sek),
        };

        let change = value_change(&current, &previous).unwrap();
        assert!(change.change_pct.is_none());
    }

    #[test]
    fn summary_across_periods() {
        let usd = Currency::new("USD");
        let sek = Currency::new("SEK");
        let aapl = InstrumentId::new("AAPL");
        let alice = UserId::new("alice");
        let acct = AccountId::new("alice-usd");

        let today = NaiveDate::from_ymd_opt(2025, 3, 15).unwrap();
        let day_ago = today - chrono::Days::new(1);
        let week_ago = today - chrono::Days::new(7);
        let year_ago = NaiveDate::from_ymd_opt(2024, 3, 15).unwrap();
        let prev_year_end = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();

        // Trade placed well before all comparison dates
        let trade_date = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let trades = vec![Trade {
            user_id: alice,
            account_id: acct,
            instrument_id: aapl.clone(),
            quantity: Quantity::new(100.0),
            price: Price::new(150.0),
            currency: usd,
            date: trade_date,
        }];

        let mut market_data = InMemoryMarketDataService::new();
        // Prices at each point in time (AAPL rising)
        market_data.add_price(&aapl, today, Price::new(200.0));
        market_data.add_price(&aapl, day_ago, Price::new(198.0));
        market_data.add_price(&aapl, week_ago, Price::new(190.0));
        market_data.add_price(&aapl, year_ago, Price::new(160.0));
        market_data.add_price(&aapl, prev_year_end, Price::new(180.0));

        // FX rates at each point
        for date in [today, day_ago, week_ago, year_ago, prev_year_end] {
            market_data.add_fx_rate(FxRate::new(usd, sek, 10.0), date);
        }

        let ctx = CalculationContext::new(sek, today);
        let summary = value_change_summary(&trades, &ctx, &market_data).unwrap();

        // Current: 100 * 200 * 10 = 200,000 SEK
        assert_eq!(summary.market_value.amount, 200_000.0);

        // Daily: 200k - 198k = 2k
        assert_eq!(summary.daily.change.amount, 2_000.0);

        // Weekly: 200k - 190k = 10k
        assert_eq!(summary.weekly.change.amount, 10_000.0);

        // Yearly: 200k - 160k = 40k
        assert_eq!(summary.yearly.change.amount, 40_000.0);

        // YTD: 200k - 180k = 20k (prev year end = Dec 31)
        assert_eq!(summary.ytd.change.amount, 20_000.0);
    }

    #[test]
    fn prev_year_handles_leap_year() {
        // 2024-02-29 → 2023-02-28
        let leap = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
        let result = prev_year(leap);
        assert_eq!(result, NaiveDate::from_ymd_opt(2023, 2, 28).unwrap());
    }
}
