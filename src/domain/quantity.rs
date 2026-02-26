use std::ops::Add;

/// Positive = long, negative = short.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quantity(f64);

impl Quantity {
    #[must_use]
    pub fn new(value: f64) -> Self {
        Quantity(value)
    }

    #[must_use]
    pub fn value(&self) -> f64 {
        self.0
    }

    #[must_use]
    #[allow(clippy::float_cmp)]
    pub fn is_zero(&self) -> bool {
        self.0 == 0.0
    }
}

impl Add for Quantity {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Quantity(self.0 + rhs.0)
    }
}
