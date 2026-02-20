use rust_decimal::Decimal;

/// A quantity of shares/units. Positive = long, negative = short.
#[derive(Clone, Copy, Debug, PartialEq, Eq, derive_more::Add)]
pub struct Quantity(Decimal);

impl Quantity {
    /// Create a new quantity.
    #[must_use]
    pub fn new(value: Decimal) -> Self {
        Quantity(value)
    }

    /// Returns the decimal value of this quantity.
    #[must_use]
    pub fn value(&self) -> Decimal {
        self.0
    }
}
