use chrono::NaiveDate;

use crate::domain::currency::Currency;

/// Immutable context for a single calculation run.
#[derive(Clone, Debug)]
pub struct CalculationContext {
    /// The target currency for all values.
    pub base_currency: Currency,
    /// The date at which to evaluate positions and prices.
    pub as_of_date: NaiveDate,
}

impl CalculationContext {
    /// Create a new calculation context.
    #[must_use]
    pub fn new(base_currency: Currency, as_of_date: NaiveDate) -> Self {
        CalculationContext {
            base_currency,
            as_of_date,
        }
    }
}
