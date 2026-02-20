use crate::context::CalculationContext;
use crate::domain::currency::Currency;
use crate::domain::instrument::InstrumentId;
use crate::domain::money::Money;
use crate::domain::position::Position;
use crate::domain::price::Price;
use crate::domain::quantity::Quantity;
use crate::error::CalceResult;
use crate::services::market_data::MarketDataService;

/// A position enriched with market value data — output of the market value calc.
#[derive(Clone, Debug)]
pub struct ValuedPosition {
    /// The instrument identifier.
    pub instrument_id: InstrumentId,
    /// The net quantity held.
    pub quantity: Quantity,
    /// The position's native currency.
    pub currency: Currency,
    /// The price per unit on the valuation date.
    pub price: Price,
    /// Market value in the position's native currency.
    pub market_value: Money,
    /// Market value converted to the base currency.
    pub market_value_base: Money,
}

/// Result of a market value calculation.
#[derive(Debug)]
pub struct MarketValueResult {
    /// Per-position breakdown.
    pub positions: Vec<ValuedPosition>,
    /// Total portfolio value in the base currency.
    pub total: Money,
}

/// Calculate market value for a set of positions.
///
/// Looks up prices and FX rates from the provided market data service,
/// computes per-position values, converts to base currency, and sums.
///
/// # Errors
///
/// Returns `PriceNotFound` if a position's instrument has no price.
/// Returns `FxRateNotFound` if a cross-currency position has no FX rate.
/// Returns `CurrencyMismatch` if an FX rate's direction doesn't match.
pub fn value_positions(
    positions: &[Position],
    ctx: &CalculationContext,
    market_data: &dyn MarketDataService,
) -> CalceResult<MarketValueResult> {
    let mut valued = Vec::with_capacity(positions.len());

    for pos in positions {
        let price = market_data.get_price(&pos.instrument_id, ctx.as_of_date)?;
        let market_value = Money::new(pos.quantity.value() * price.value(), pos.currency);

        let market_value_base = if pos.currency == ctx.base_currency {
            market_value
        } else {
            let rate =
                market_data.get_fx_rate(pos.currency, ctx.base_currency, ctx.as_of_date)?;
            market_value.convert(&rate)?
        };

        valued.push(ValuedPosition {
            instrument_id: pos.instrument_id.clone(),
            quantity: pos.quantity,
            currency: pos.currency,
            price,
            market_value,
            market_value_base,
        });
    }

    // Sort for deterministic output
    valued.sort_by(|a, b| a.instrument_id.as_str().cmp(b.instrument_id.as_str()));

    let total = valued
        .iter()
        .try_fold(Money::zero(ctx.base_currency), |acc, p| {
            acc.checked_add(p.market_value_base)
        })?;

    Ok(MarketValueResult {
        positions: valued,
        total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::currency::Currency;
    use crate::domain::fx_rate::FxRate;
    use crate::domain::quantity::Quantity;
    use crate::services::market_data::InMemoryMarketDataService;
    use chrono::NaiveDate;
    use rust_decimal_macros::dec;

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2025, 1, 15).expect("valid test date")
    }

    #[test]
    fn single_position_same_currency() {
        let usd = Currency::new("USD");
        let aapl = InstrumentId::new("AAPL");

        let mut market_data = InMemoryMarketDataService::new();
        market_data.add_price(&aapl, date(), Price::new(dec!(150)));

        let positions = vec![Position {
            instrument_id: aapl,
            quantity: Quantity::new(dec!(100)),
            currency: usd,
        }];
        let ctx = CalculationContext::new(usd, date());

        let result = value_positions(&positions, &ctx, &market_data).expect("should succeed");
        assert_eq!(result.total.amount, dec!(15000));
        assert_eq!(result.total.currency, usd);
    }

    #[test]
    fn cross_currency_conversion() {
        let usd = Currency::new("USD");
        let sek = Currency::new("SEK");
        let aapl = InstrumentId::new("AAPL");

        let mut market_data = InMemoryMarketDataService::new();
        market_data.add_price(&aapl, date(), Price::new(dec!(150)));
        market_data.add_fx_rate(FxRate::new(usd, sek, dec!(10)), date());

        let positions = vec![Position {
            instrument_id: aapl,
            quantity: Quantity::new(dec!(10)),
            currency: usd,
        }];
        let ctx = CalculationContext::new(sek, date());

        let result = value_positions(&positions, &ctx, &market_data).expect("should succeed");
        // 10 * 150 = 1500 USD -> 1500 * 10 = 15000 SEK
        assert_eq!(result.total.amount, dec!(15000));
        assert_eq!(result.total.currency, sek);
    }

    #[test]
    fn missing_price_returns_error() {
        let usd = Currency::new("USD");
        let aapl = InstrumentId::new("AAPL");
        let market_data = InMemoryMarketDataService::new();

        let positions = vec![Position {
            instrument_id: aapl,
            quantity: Quantity::new(dec!(100)),
            currency: usd,
        }];
        let ctx = CalculationContext::new(usd, date());

        let result = value_positions(&positions, &ctx, &market_data);
        assert!(result.is_err());
    }

    #[test]
    fn empty_positions_returns_zero() {
        let sek = Currency::new("SEK");
        let market_data = InMemoryMarketDataService::new();
        let ctx = CalculationContext::new(sek, date());

        let result = value_positions(&[], &ctx, &market_data).expect("should succeed");
        assert_eq!(result.total.amount, dec!(0));
        assert!(result.positions.is_empty());
    }
}
