use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Price(f64);

impl Price {
    #[must_use]
    pub fn new(value: f64) -> Self {
        Price(value)
    }

    #[must_use]
    pub fn value(&self) -> f64 {
        self.0
    }
}

impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
