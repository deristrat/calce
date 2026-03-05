use calce_core::error::CalceError;

pub type DataResult<T> = Result<T, DataError>;

#[derive(Debug, thiserror::Error)]
pub enum DataError {
    #[error("Database error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("Migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("{0}")]
    Core(#[from] CalceError),
}

impl From<DataError> for CalceError {
    fn from(err: DataError) -> Self {
        match err {
            DataError::Core(e) => e,
            other => CalceError::DataError(other.to_string()),
        }
    }
}
