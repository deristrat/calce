use calce_core::domain::user::UserId;
use calce_core::error::CalceError;
use chrono::{DateTime, Utc};
use sqlx::Error as SqlxError;

pub type DataResult<T> = Result<T, DataError>;

#[derive(Debug, thiserror::Error)]
pub enum DataError {
    #[error("Unauthorized: user {requester} cannot access data for user {target}")]
    Unauthorized { requester: UserId, target: UserId },

    #[error("No trades found for user {0}")]
    NoTradesFound(UserId),

    #[error("Database error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("{0}")]
    Calc(#[from] CalceError),

    #[error("Invalid data from DB: column {column}, value {value}: {reason}")]
    InvalidDbData {
        column: String,
        value: String,
        reason: String,
    },

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("{0}")]
    Conflict(String),

    #[error("Invalid email or password")]
    InvalidCredentials,

    #[error("Account locked until {retry_after}")]
    AccountLocked { retry_after: DateTime<Utc> },

    #[error("Invalid or expired refresh token")]
    InvalidRefreshToken,

    #[error("Refresh token reuse detected — session revoked")]
    TokenReplayDetected,
}

impl DataError {
    /// Convert a sqlx error into a `Conflict` if it's a unique or FK violation,
    /// otherwise fall through to a generic `Sqlx` error.
    pub(crate) fn from_constraint_violation(err: SqlxError, entity: &str, id: &str) -> Self {
        if let SqlxError::Database(ref db_err) = err {
            if db_err.is_unique_violation() {
                return Self::Conflict(format!("{entity} '{id}' already exists"));
            }
            if db_err.is_foreign_key_violation() {
                return Self::Conflict(format!(
                    "cannot delete {entity} '{id}': has dependent records"
                ));
            }
        }
        Self::from(err)
    }
}
