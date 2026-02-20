use std::fmt;

/// Unique identifier for a financial instrument (e.g. "AAPL", "VOW3").
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InstrumentId(String);

impl InstrumentId {
    /// Create a new instrument identifier.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        InstrumentId(id.into())
    }

    /// Returns the identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for InstrumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
