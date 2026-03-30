pub mod error;
pub mod listener;
pub mod protocol;
pub mod wire;

pub use error::CdcError;
pub use listener::CdcListener;

use calce_core::domain::currency::Currency;
use calce_core::domain::instrument::InstrumentId;
use chrono::NaiveDate;

/// CDC listener configuration.
pub struct CdcConfig {
    pub database_url: String,
    pub slot_name: String,
    pub publication_name: String,
}

impl CdcConfig {
    /// Build from environment, or `None` if CDC is disabled.
    ///
    /// Reads `CALCE_CDC_ENABLED` (default: true) and `DATABASE_URL`.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let enabled = std::env::var("CALCE_CDC_ENABLED")
            .map(|v| !matches!(v.as_str(), "false" | "0"))
            .unwrap_or(true);
        if !enabled {
            return None;
        }
        let database_url = std::env::var("DATABASE_URL").ok()?;
        Some(Self {
            database_url,
            slot_name: "calce_cdc_slot".into(),
            publication_name: "calce_cdc_pub".into(),
        })
    }
}

/// A typed change event from the database.
#[derive(Debug, Clone)]
pub enum CdcEvent {
    /// A price was inserted or updated.
    PriceChanged {
        instrument_id: InstrumentId,
        date: NaiveDate,
        price: f64,
    },
    /// An FX rate was inserted or updated.
    FxRateChanged {
        from_currency: Currency,
        to_currency: Currency,
        date: NaiveDate,
        rate: f64,
    },
}
