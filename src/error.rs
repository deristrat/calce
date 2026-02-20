use chrono::NaiveDate;

use crate::domain::currency::Currency;
use crate::domain::instrument::InstrumentId;
use crate::domain::money::CurrencyMismatch;
use crate::domain::user::UserId;

/// Alias for `Result<T, CalceError>`.
pub type CalceResult<T> = Result<T, CalceError>;

/// Top-level error type aggregating all domain and service errors.
#[derive(Debug, thiserror::Error)]
pub enum CalceError {
    /// The requesting user lacks permission to access the target user's data.
    #[error("Unauthorized: user {requester} cannot access data for user {target}")]
    Unauthorized {
        /// The user who made the request.
        requester: UserId,
        /// The user whose data was requested.
        target: UserId,
    },

    /// No price data available for the given instrument and date.
    #[error("Price not found for {instrument} on {date}")]
    PriceNotFound {
        /// The instrument that was looked up.
        instrument: InstrumentId,
        /// The date for which the price was requested.
        date: NaiveDate,
    },

    /// No FX rate available for the given currency pair and date.
    #[error("FX rate not found for {from}/{to} on {date}")]
    FxRateNotFound {
        /// Source currency.
        from: Currency,
        /// Target currency.
        to: Currency,
        /// The date for which the rate was requested.
        date: NaiveDate,
    },

    /// The user has no trades on record.
    #[error("No trades found for user {0}")]
    NoTradesFound(UserId),

    /// An FX conversion was attempted with mismatched currencies.
    #[error("Currency mismatch: {0}")]
    CurrencyMismatch(#[from] CurrencyMismatch),
}
