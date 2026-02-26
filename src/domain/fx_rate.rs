use std::fmt;

use super::currency::Currency;

/// Directed exchange rate: `FxRate { from: USD, to: SEK, rate: 10.5 }` means
/// 1 USD = 10.5 SEK. The rate carries its currency pair so that multiplying
/// Money(USD) by FxRate(USD->SEK) "cancels" USD and produces SEK.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxRate {
    pub from: Currency,
    pub to: Currency,
    /// 1 unit of `from` = `rate` units of `to`.
    pub rate: f64,
}

impl FxRate {
    #[must_use]
    pub fn new(from: Currency, to: Currency, rate: f64) -> Self {
        FxRate { from, to, rate }
    }

    #[must_use]
    pub fn identity(currency: Currency) -> Self {
        FxRate {
            from: currency,
            to: currency,
            rate: 1.0,
        }
    }

    #[must_use]
    pub fn invert(&self) -> Self {
        FxRate {
            from: self.to,
            to: self.from,
            rate: 1.0 / self.rate,
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

    #[test]
    fn invert_swaps_currencies() {
        let usd = Currency::new("USD");
        let sek = Currency::new("SEK");
        let rate = FxRate::new(usd, sek, 10.0);
        let inv = rate.invert();
        assert_eq!(inv.from, sek);
        assert_eq!(inv.to, usd);
        assert_eq!(inv.rate, 0.1);
    }

    #[test]
    fn identity_is_one() {
        let usd = Currency::new("USD");
        let id = FxRate::identity(usd);
        assert_eq!(id.from, usd);
        assert_eq!(id.to, usd);
        assert_eq!(id.rate, 1.0);
    }
}
