use crate::calc::aggregation::aggregate_positions;
use crate::calc::market_value::{value_positions, MarketValueResult};
use crate::calc::value_change::{value_change_summary_from, ValueChangeSummary};
use crate::context::CalculationContext;
use crate::domain::trade::Trade;
use crate::error::CalceResult;
use crate::services::market_data::MarketDataService;

/// `#CALC_REPORT`
///
/// Bundled portfolio view: market value + value changes in one pass.
///
/// Aggregates trades once, computes the current-date market value, then
/// feeds that snapshot into `value_change_summary_from` so the current-date
/// valuation is never duplicated.
#[derive(Debug)]
pub struct PortfolioReport {
    pub market_value: MarketValueResult,
    pub value_changes: ValueChangeSummary,
}

/// `#CALC_REPORT` — pure composite: aggregate, value, diff in one call.
///
/// # Errors
///
/// Propagates errors from position aggregation, market value, or value change.
pub fn portfolio_report(
    trades: &[Trade],
    ctx: &CalculationContext,
    market_data: &dyn MarketDataService,
) -> CalceResult<PortfolioReport> {
    let positions = aggregate_positions(trades, ctx.as_of_date);
    let market_value = value_positions(&positions, ctx, market_data)?;
    let value_changes = value_change_summary_from(&market_value, trades, ctx, market_data)?;

    Ok(PortfolioReport {
        market_value,
        value_changes,
    })
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
    use chrono::NaiveDate;

    #[test]
    fn report_contains_mv_and_value_changes() {
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
        market_data.add_price(&aapl, today, Price::new(200.0));
        market_data.add_price(&aapl, day_ago, Price::new(198.0));
        market_data.add_price(&aapl, week_ago, Price::new(190.0));
        market_data.add_price(&aapl, year_ago, Price::new(160.0));
        market_data.add_price(&aapl, prev_year_end, Price::new(180.0));

        for date in [today, day_ago, week_ago, year_ago, prev_year_end] {
            market_data.add_fx_rate(FxRate::new(usd, sek, 10.0), date);
        }

        let ctx = CalculationContext::new(sek, today);
        let report = portfolio_report(&trades, &ctx, &market_data).unwrap();

        // Market value: 100 * 200 * 10 = 200,000 SEK
        assert_eq!(report.market_value.total.amount, 200_000.0);
        assert_eq!(report.market_value.positions.len(), 1);

        // Value changes match value_change_summary
        assert_eq!(report.value_changes.market_value.amount, 200_000.0);
        assert_eq!(report.value_changes.daily.change.amount, 2_000.0);
        assert_eq!(report.value_changes.weekly.change.amount, 10_000.0);
        assert_eq!(report.value_changes.yearly.change.amount, 40_000.0);
        assert_eq!(report.value_changes.ytd.change.amount, 20_000.0);
    }
}
