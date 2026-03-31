use crate::context::CalculationContext;
use crate::domain::currency::Currency;
use crate::domain::instrument::InstrumentId;
use crate::domain::money::Money;
use crate::domain::position::Position;
use crate::domain::price::Price;
use crate::domain::quantity::Quantity;
use crate::error::CalceResult;
use crate::outcome::{Outcome, Warning};
use crate::services::market_data::MarketDataService;

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ValuedPosition {
    pub instrument_id: InstrumentId,
    pub quantity: Quantity,
    pub currency: Currency,
    pub price: Price,
    pub market_value: Money,
    pub market_value_base: Money,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MarketValueResult {
    pub positions: Vec<ValuedPosition>,
    pub total: Money,
}

/// `#CALC_MV`
///
/// Values each position at current market prices. Positions where the price
/// or FX rate is missing are skipped and reported as warnings rather than
/// failing the entire calculation.
///
/// # Errors
///
/// Returns `CurrencyMismatch` if an FX rate's direction doesn't match (a bug,
/// not missing data). All missing-data situations produce warnings instead.
pub fn value_positions(
    positions: &[Position],
    ctx: &CalculationContext,
    market_data: &dyn MarketDataService,
) -> CalceResult<Outcome<MarketValueResult>> {
    let mut valued = Vec::with_capacity(positions.len());
    let mut warnings = Vec::new();

    for pos in positions {
        let price = match market_data.get_price(&pos.instrument_id, ctx.as_of_date) {
            Ok(p) => p,
            Err(e) => {
                warnings.push(Warning::missing_price(e.to_string()));
                continue;
            }
        };

        let market_value = Money::new(pos.quantity.value() * price.value(), pos.currency);

        let market_value_base = if pos.currency == ctx.base_currency {
            market_value
        } else {
            let rate =
                match market_data.get_fx_rate(pos.currency, ctx.base_currency, ctx.as_of_date) {
                    Ok(r) => r,
                    Err(e) => {
                        warnings.push(Warning::missing_fx_rate(e.to_string()));
                        continue;
                    }
                };
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

    Ok(Outcome::with_warnings(
        MarketValueResult {
            positions: valued,
            total,
        },
        warnings,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::currency::Currency;
    use crate::domain::fx_rate::FxRate;
    use crate::domain::quantity::Quantity;
    use crate::services::test_market_data::TestMarketData;
    use chrono::NaiveDate;

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2025, 1, 15).expect("valid test date")
    }

    #[test]
    fn single_position_same_currency() {
        let usd = Currency::new("USD");
        let aapl = InstrumentId::new("AAPL");

        let mut market_data = TestMarketData::new();
        market_data.add_price(&aapl, date(), Price::new(150.0));

        let positions = vec![Position {
            instrument_id: aapl,
            quantity: Quantity::new(100.0),
            currency: usd,
        }];
        let ctx = CalculationContext::new(usd, date());

        let outcome = value_positions(&positions, &ctx, &market_data).expect("should succeed");
        assert!(!outcome.has_warnings());
        assert_eq!(outcome.value.total.amount, 15000.0);
        assert_eq!(outcome.value.total.currency, usd);
    }

    #[test]
    fn cross_currency_conversion() {
        let usd = Currency::new("USD");
        let sek = Currency::new("SEK");
        let aapl = InstrumentId::new("AAPL");

        let mut market_data = TestMarketData::new();
        market_data.add_price(&aapl, date(), Price::new(150.0));
        market_data.add_fx_rate(FxRate::new(usd, sek, 10.0), date());

        let positions = vec![Position {
            instrument_id: aapl,
            quantity: Quantity::new(10.0),
            currency: usd,
        }];
        let ctx = CalculationContext::new(sek, date());

        let outcome = value_positions(&positions, &ctx, &market_data).expect("should succeed");
        // 10 * 150 = 1500 USD -> 1500 * 10 = 15000 SEK
        assert_eq!(outcome.value.total.amount, 15000.0);
        assert_eq!(outcome.value.total.currency, sek);
    }

    #[test]
    fn missing_price_produces_warning_not_error() {
        let usd = Currency::new("USD");
        let aapl = InstrumentId::new("AAPL");
        let market_data = TestMarketData::new();

        let positions = vec![Position {
            instrument_id: aapl,
            quantity: Quantity::new(100.0),
            currency: usd,
        }];
        let ctx = CalculationContext::new(usd, date());

        let outcome = value_positions(&positions, &ctx, &market_data).expect("should succeed");
        assert!(outcome.has_warnings());
        assert_eq!(outcome.warnings.len(), 1);
        assert_eq!(outcome.value.positions.len(), 0);
        assert_eq!(outcome.value.total.amount, 0.0);
    }

    #[test]
    fn partial_success_with_mixed_availability() {
        let usd = Currency::new("USD");
        let aapl = InstrumentId::new("AAPL");
        let msft = InstrumentId::new("MSFT");

        let mut market_data = TestMarketData::new();
        // Only AAPL has a price; MSFT is missing
        market_data.add_price(&aapl, date(), Price::new(150.0));

        let positions = vec![
            Position {
                instrument_id: aapl,
                quantity: Quantity::new(100.0),
                currency: usd,
            },
            Position {
                instrument_id: msft,
                quantity: Quantity::new(50.0),
                currency: usd,
            },
        ];
        let ctx = CalculationContext::new(usd, date());

        let outcome = value_positions(&positions, &ctx, &market_data).expect("should succeed");
        // AAPL valued, MSFT skipped with warning
        assert_eq!(outcome.value.positions.len(), 1);
        assert_eq!(outcome.value.total.amount, 15000.0);
        assert_eq!(outcome.warnings.len(), 1);
    }

    #[test]
    fn empty_positions_returns_zero() {
        let sek = Currency::new("SEK");
        let market_data = TestMarketData::new();

        let ctx = CalculationContext::new(sek, date());

        let outcome = value_positions(&[], &ctx, &market_data).expect("should succeed");
        assert_eq!(outcome.value.total.amount, 0.0);
        assert!(outcome.value.positions.is_empty());
        assert!(!outcome.has_warnings());
    }
}
