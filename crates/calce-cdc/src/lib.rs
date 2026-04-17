//! Postgres logical-replication listener for Calce.
//!
//! Streams WAL changes through the `pgoutput` plugin and emits [`CdcEvent`]s
//! on a Tokio channel. Each event carries the table name, the DML operation,
//! and the row's columns as text — consumers decode the domain meaning.
//!
//! The listener creates (or reuses) a replication slot and publication on
//! startup, reconnects with exponential backoff on failure, and
//! back-pressures the WAL stream when the consumer is slow.
//!
//! ```no_run
//! # async fn run() {
//! let config = calce_cdc::CdcConfig::from_env().expect("CDC disabled");
//! let (listener, mut rx) = calce_cdc::CdcListener::new(config, 4096);
//! tokio::spawn(listener.run());
//! while let Some(event) = rx.recv().await {
//!     // handle event
//! }
//! # }
//! ```

mod error;
mod listener;
mod protocol;
mod wire;

pub use error::CdcError;
pub use listener::CdcListener;

use std::collections::HashMap;

/// Tables included in the CDC publication.
///
/// The listener creates or amends the publication on startup so exactly these
/// tables are replicated. Events for any other table are never emitted.
pub const REPLICATED_TABLES: &[&str] = &[
    "prices",
    "fx_rates",
    "trades",
    "instruments",
    "users",
    "organizations",
    "accounts",
    "api_keys",
];

/// The kind of DML operation that triggered a CDC event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdcOperation {
    Insert,
    Update,
    Delete,
}

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

/// A single row change replicated from Postgres.
///
/// For `Delete`, `columns` contains only the primary-key or replica-identity
/// columns; for `Insert`/`Update` it contains the full new row.
#[derive(Debug, Clone)]
pub struct CdcEvent {
    pub table: String,
    pub operation: CdcOperation,
    /// Column name → text value. `None` means NULL or an unchanged TOAST value.
    pub columns: HashMap<String, Option<String>>,
}
