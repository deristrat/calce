use rust_decimal::Decimal;

/// The price of one unit of an instrument.
#[derive(Clone, Copy, Debug, PartialEq, Eq, derive_more::Display)]
pub struct Price(Decimal);

impl Price {
    /// Create a new price.
    #[must_use]
    pub fn new(value: Decimal) -> Self {
        Price(value)
    }

    /// Returns the decimal value of this price.
    #[must_use]
    pub fn value(&self) -> Decimal {
        self.0
    }
}
