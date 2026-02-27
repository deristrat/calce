use rust_decimal::Decimal;

use crate::domain::currency::Currency;

/// Exact-precision balance for ledger accounting. Uses `Decimal` because
/// accounting arithmetic must be exact (debits and credits must balance to zero).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Balance {
    pub amount: Decimal,
    pub currency: Currency,
}

pub struct LedgerEntry {
    pub amount: Decimal,
    pub currency: Currency,
    pub description: String,
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("expected currency {expected}, got {actual}")]
pub struct MixedCurrency {
    pub expected: Currency,
    pub actual: Currency,
}

/// `#CALC_LEDGER_BAL`
///
/// Sum ledger entries into a single balance. All entries must share the
/// expected currency.
///
/// # Errors
///
/// Returns `MixedCurrency` if any entry's currency differs from `expected_currency`.
pub fn sum_entries(
    entries: &[LedgerEntry],
    expected_currency: Currency,
) -> Result<Balance, MixedCurrency> {
    let mut total = Decimal::ZERO;
    for entry in entries {
        if entry.currency != expected_currency {
            return Err(MixedCurrency {
                expected: expected_currency,
                actual: entry.currency,
            });
        }
        total += entry.amount;
    }
    Ok(Balance {
        amount: total,
        currency: expected_currency,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn sum_entries_exact_arithmetic() {
        let usd = Currency::new("USD");
        let entries = vec![
            LedgerEntry {
                amount: dec!(100.50),
                currency: usd,
                description: "deposit".into(),
            },
            LedgerEntry {
                amount: dec!(-25.30),
                currency: usd,
                description: "withdrawal".into(),
            },
            LedgerEntry {
                amount: dec!(0.01),
                currency: usd,
                description: "interest".into(),
            },
        ];
        let balance = sum_entries(&entries, usd).expect("same currency");
        assert_eq!(balance.amount, dec!(75.21));
        assert_eq!(balance.currency, usd);
    }

    #[test]
    fn sum_entries_empty() {
        let usd = Currency::new("USD");
        let balance = sum_entries(&[], usd).expect("empty is fine");
        assert_eq!(balance.amount, Decimal::ZERO);
    }

    #[test]
    fn sum_entries_mixed_currency_rejected() {
        let usd = Currency::new("USD");
        let eur = Currency::new("EUR");
        let entries = vec![
            LedgerEntry {
                amount: dec!(100.0),
                currency: usd,
                description: "deposit".into(),
            },
            LedgerEntry {
                amount: dec!(50.0),
                currency: eur,
                description: "wrong currency".into(),
            },
        ];
        let err = sum_entries(&entries, usd).unwrap_err();
        assert_eq!(err.expected, usd);
        assert_eq!(err.actual, eur);
    }
}
