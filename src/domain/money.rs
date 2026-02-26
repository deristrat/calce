use std::fmt;

use super::currency::Currency;
use super::fx_rate::FxRate;

/// E.g. converting Money(EUR) with FxRate(USD->SEK) is a mismatch.
#[derive(Debug, Clone, thiserror::Error)]
#[error("FX rate expects {expected}, but money is in {actual}")]
pub struct CurrencyMismatch {
    pub expected: Currency,
    pub actual: Currency,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Money {
    pub amount: f64,
    pub currency: Currency,
}

impl Money {
    #[must_use]
    pub fn new(amount: f64, currency: Currency) -> Self {
        Money { amount, currency }
    }

    #[must_use]
    pub fn zero(currency: Currency) -> Self {
        Money {
            amount: 0.0,
            currency,
        }
    }

    /// # Errors
    ///
    /// Returns `CurrencyMismatch` if currencies differ.
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

    /// Convert using a directed FX rate. Validates that this Money's currency
    /// matches the rate's `from` currency; result is in the rate's `to` currency.
    ///
    /// # Errors
    ///
    /// Returns `CurrencyMismatch` if `self.currency != rate.from`.
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

    #[test]
    fn convert_applies_directed_fx_rate() {
        let usd = Currency::new("USD");
        let sek = Currency::new("SEK");
        let money = Money::new(100.0, usd);
        let rate = FxRate::new(usd, sek, 10.5);
        let converted = money.convert(&rate).expect("valid conversion");
        assert_eq!(converted.amount, 1050.0);
        assert_eq!(converted.currency, sek);
    }

    #[test]
    fn convert_rejects_wrong_currency() {
        let usd = Currency::new("USD");
        let eur = Currency::new("EUR");
        let sek = Currency::new("SEK");
        let money = Money::new(100.0, eur);
        let rate = FxRate::new(usd, sek, 10.5);
        let err = money.convert(&rate).unwrap_err();
        assert_eq!(err.expected, usd);
        assert_eq!(err.actual, eur);
    }

    #[test]
    fn checked_add_same_currency() {
        let usd = Currency::new("USD");
        let a = Money::new(100.0, usd);
        let b = Money::new(200.0, usd);
        let sum = a.checked_add(b).expect("same currency");
        assert_eq!(sum.amount, 300.0);
        assert_eq!(sum.currency, usd);
    }

    #[test]
    fn checked_add_different_currencies_returns_error() {
        let a = Money::new(100.0, Currency::new("USD"));
        let b = Money::new(200.0, Currency::new("EUR"));
        assert!(a.checked_add(b).is_err());
    }
}
