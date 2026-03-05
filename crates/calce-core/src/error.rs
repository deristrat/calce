use chrono::NaiveDate;

use crate::domain::currency::Currency;
use crate::domain::instrument::InstrumentId;
use crate::domain::money::CurrencyMismatch;
use crate::domain::user::UserId;

pub type CalceResult<T> = Result<T, CalceError>;

#[derive(Debug, thiserror::Error)]
pub enum CalceError {
    #[error("Unauthorized: user {requester} cannot access data for user {target}")]
    Unauthorized { requester: UserId, target: UserId },

    #[error("Price not found for {instrument} on {date}")]
    PriceNotFound {
        instrument: InstrumentId,
        date: NaiveDate,
    },

    #[error("FX rate not found for {from}/{to} on {date}")]
    FxRateNotFound {
        from: Currency,
        to: Currency,
        date: NaiveDate,
    },

    #[error("No trades found for user {0}")]
    NoTradesFound(UserId),

    #[error("Insufficient data for {instrument}: {reason}")]
    InsufficientData {
        instrument: InstrumentId,
        reason: String,
    },

    #[error("Currency mismatch: {0}")]
    CurrencyMismatch(#[from] CurrencyMismatch),

    #[error("Data error: {0}")]
    DataError(String),
}
