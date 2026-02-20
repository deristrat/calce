use rust_decimal::Decimal;
use std::fmt;

use super::currency::Currency;
use super::fx_rate::FxRate;

/// Error when an FX conversion is attempted with mismatched currencies.
/// E.g. trying to convert Money(EUR) using FxRate(USD->SEK).
#[derive(Debug, Clone, thiserror::Error)]
#[error("FX rate expects {expected}, but money is in {actual}")]
pub struct CurrencyMismatch {
    /// The currency the FX rate expected.
    pub expected: Currency,
    /// The actual currency of the money.
    pub actual: Currency,
}

/// A monetary amount in a specific currency.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Money {
    /// The decimal amount.
    pub amount: Decimal,
    /// The currency of this amount.
    pub currency: Currency,
}

impl Money {
    /// Create a new monetary amount.
    #[must_use]
    pub fn new(amount: Decimal, currency: Currency) -> Self {
        Money { amount, currency }
    }

    /// Create a zero amount in the given currency.
    #[must_use]
    pub fn zero(currency: Currency) -> Self {
        Money {
            amount: Decimal::ZERO,
            currency,
        }
    }

    /// Add two monetary amounts, returning an error if currencies differ.
    ///
    /// # Errors
    ///
    /// Returns `CurrencyMismatch` if `self` and `other` have different currencies.
    pub fn checked_add(self, other: Self) -> Result<Money, CurrencyMismatch> {
        if self.currency != other.currency {
            return Err(CurrencyMismatch {
                expected: self.currency,
                actual: other.currency,
            });
        }
        Ok(Money {
            amount: self.amount + other.amount,
            currency: self.currency,
        })
    }

    /// Convert to another currency using a directed FX rate.
    ///
    /// Validates that this Money's currency matches the rate's `from` currency.
    /// The result will be in the rate's `to` currency.
    ///
    /// # Errors
    ///
    /// Returns `CurrencyMismatch` if `self.currency` does not match `rate.from`.
    pub fn convert(&self, rate: &FxRate) -> Result<Money, CurrencyMismatch> {
        if self.currency != rate.from {
            return Err(CurrencyMismatch {
                expected: rate.from,
                actual: self.currency,
            });
        }
        Ok(Money {
            amount: self.amount * rate.rate,
            currency: rate.to,
        })
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.amount, self.currency)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn convert_applies_directed_fx_rate() {
        let usd = Currency::new("USD");
        let sek = Currency::new("SEK");
        let money = Money::new(dec!(100), usd);
        let rate = FxRate::new(usd, sek, dec!(10.5));
        let converted = money.convert(&rate).expect("valid conversion");
        assert_eq!(converted.amount, dec!(1050.0));
        assert_eq!(converted.currency, sek);
    }

    #[test]
    fn convert_rejects_wrong_currency() {
        let usd = Currency::new("USD");
        let eur = Currency::new("EUR");
        let sek = Currency::new("SEK");
        let money = Money::new(dec!(100), eur);
        let rate = FxRate::new(usd, sek, dec!(10.5));
        let err = money.convert(&rate).unwrap_err();
        assert_eq!(err.expected, usd);
        assert_eq!(err.actual, eur);
    }

    #[test]
    fn checked_add_same_currency() {
        let usd = Currency::new("USD");
        let a = Money::new(dec!(100), usd);
        let b = Money::new(dec!(200), usd);
        let sum = a.checked_add(b).expect("same currency");
        assert_eq!(sum.amount, dec!(300));
        assert_eq!(sum.currency, usd);
    }

    #[test]
    fn checked_add_different_currencies_returns_error() {
        let a = Money::new(dec!(100), Currency::new("USD"));
        let b = Money::new(dec!(200), Currency::new("EUR"));
        assert!(a.checked_add(b).is_err());
    }
}
