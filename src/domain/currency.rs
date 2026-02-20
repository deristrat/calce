use std::fmt;
use std::str::FromStr;

/// Error for invalid currency codes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("Currency code must be exactly 3 uppercase ASCII letters")]
pub struct InvalidCurrencyCode;

/// ISO 4217 currency code stored as 3 ASCII bytes.
/// Copy, stack-allocated, cheap to pass around.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Currency([u8; 3]);

impl Currency {
    /// Create a currency from a known-valid code.
    /// Use `try_new` or `parse` for runtime data.
    ///
    /// # Panics
    ///
    /// Panics if `code` is not exactly 3 uppercase ASCII letters.
    #[allow(clippy::expect_used)]
    #[must_use]
    pub fn new(code: &str) -> Self {
        Self::try_new(code).expect("Invalid currency code")
    }

    /// Fallible constructor for runtime data.
    ///
    /// # Errors
    ///
    /// Returns `InvalidCurrencyCode` if the code is not exactly 3 uppercase ASCII letters.
    pub fn try_new(code: &str) -> Result<Self, InvalidCurrencyCode> {
        let bytes: [u8; 3] = code
            .as_bytes()
            .try_into()
            .map_err(|_| InvalidCurrencyCode)?;

        if bytes.iter().all(u8::is_ascii_uppercase) {
            Ok(Currency(bytes))
        } else {
            Err(InvalidCurrencyCode)
        }
    }

    /// Returns the currency code as a string slice.
    ///
    /// # Panics
    ///
    /// Cannot panic — bytes are validated at construction.
    #[allow(clippy::expect_used)]
    #[must_use]
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("Currency always contains valid ASCII")
    }
}

impl FromStr for Currency {
    type Err = InvalidCurrencyCode;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_new(s)
    }
}

impl AsRef<str> for Currency {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Debug for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Currency({})", self.as_str())
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn currency_roundtrip() {
        let usd = Currency::new("USD");
        assert_eq!(usd.as_str(), "USD");
    }

    #[test]
    fn currency_from_str() {
        let eur: Currency = "EUR".parse().expect("valid currency");
        assert_eq!(eur.as_str(), "EUR");
    }

    #[test]
    fn currency_is_copy() {
        let a = Currency::new("SEK");
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn rejects_invalid_length() {
        assert!(Currency::try_new("US").is_err());
        assert!(Currency::try_new("USDA").is_err());
    }

    #[test]
    fn rejects_lowercase_and_non_alpha() {
        assert!(Currency::try_new("usd").is_err());
        assert!(Currency::try_new("US1").is_err());
        assert!(Currency::try_new("$%^").is_err());
    }
}
