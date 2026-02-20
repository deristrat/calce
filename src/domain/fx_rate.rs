use rust_decimal::Decimal;
use std::fmt;

use super::currency::Currency;

/// A directed exchange rate between two currencies.
///
/// `FxRate { from: USD, to: SEK, rate: 10.5 }` means "1 USD = 10.5 SEK".
/// Think of it as a unit: the rate carries its currency pair, enabling
/// validation on conversion. Multiplying Money(USD) by FxRate(USD->SEK)
/// "cancels" USD and produces SEK.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FxRate {
    /// The source currency.
    pub from: Currency,
    /// The target currency.
    pub to: Currency,
    /// The exchange rate (1 unit of `from` = `rate` units of `to`).
    pub rate: Decimal,
}

impl FxRate {
    /// Create a new directed FX rate.
    #[must_use]
    pub fn new(from: Currency, to: Currency, rate: Decimal) -> Self {
        FxRate { from, to, rate }
    }

    /// Identity rate (1.0) for same-currency "conversion".
    #[must_use]
    pub fn identity(currency: Currency) -> Self {
        FxRate {
            from: currency,
            to: currency,
            rate: Decimal::ONE,
        }
    }

    /// Invert the rate: USD->SEK becomes SEK->USD.
    #[must_use]
    pub fn invert(&self) -> Self {
        FxRate {
            from: self.to,
            to: self.from,
            rate: Decimal::ONE / self.rate,
        }
    }
}

impl fmt::Display for FxRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{} {}", self.from, self.to, self.rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn invert_swaps_currencies() {
        let usd = Currency::new("USD");
        let sek = Currency::new("SEK");
        let rate = FxRate::new(usd, sek, dec!(10));
        let inv = rate.invert();
        assert_eq!(inv.from, sek);
        assert_eq!(inv.to, usd);
        assert_eq!(inv.rate, dec!(0.1));
    }

    #[test]
    fn identity_is_one() {
        let usd = Currency::new("USD");
        let id = FxRate::identity(usd);
        assert_eq!(id.from, usd);
        assert_eq!(id.to, usd);
        assert_eq!(id.rate, Decimal::ONE);
    }
}
