use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use calce_core::error::CalceError;
use calce_data::error::DataError;
use serde_json::json;

pub(crate) enum ApiError {
    Data(DataError),
    Calc(CalceError),
    BadRequest(String),
    RateLimited { retry_after_secs: u64 },
}

impl From<DataError> for ApiError {
    fn from(err: DataError) -> Self {
        ApiError::Data(err)
    }
}

impl From<CalceError> for ApiError {
    fn from(err: CalceError) -> Self {
        ApiError::Calc(err)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message, extra_headers) = match &self {
            ApiError::BadRequest(msg) => {
                (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg.clone(), None)
            }
            ApiError::RateLimited { retry_after_secs } => (
                StatusCode::TOO_MANY_REQUESTS,
                "RATE_LIMITED",
                "Too many requests. Try again later.".to_owned(),
                Some((header::RETRY_AFTER, retry_after_secs.to_string())),
            ),
            ApiError::Data(err) => {
                let (s, c, m) = data_error_response(err);
                (s, c, m, None)
            }
            ApiError::Calc(err) => {
                let (s, c, m) = calc_error_response(err);
                (s, c, m, None)
            }
        };

        let body = json!({ "error": code, "message": message });
        let mut response = (status, axum::Json(body)).into_response();
        if let Some((key, value)) = extra_headers
            && let Ok(hv) = HeaderValue::from_str(&value)
        {
            response.headers_mut().insert(key, hv);
        }
        response
    }
}

fn data_error_response(err: &DataError) -> (StatusCode, &'static str, String) {
    match err {
        DataError::Unauthorized { .. } => (StatusCode::FORBIDDEN, "UNAUTHORIZED", err.to_string()),
        DataError::NoTradesFound(_) => (StatusCode::NOT_FOUND, "NO_TRADES_FOUND", err.to_string()),
        DataError::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND", err.to_string()),
        DataError::Conflict(_) => (StatusCode::CONFLICT, "CONFLICT", err.to_string()),
        DataError::InvalidCredentials => (
            StatusCode::UNAUTHORIZED,
            "INVALID_CREDENTIALS",
            err.to_string(),
        ),
        DataError::AccountLocked { .. } => (StatusCode::LOCKED, "ACCOUNT_LOCKED", err.to_string()),
        DataError::InvalidRefreshToken => (
            StatusCode::UNAUTHORIZED,
            "INVALID_REFRESH_TOKEN",
            err.to_string(),
        ),
        DataError::TokenReplayDetected => (
            StatusCode::UNAUTHORIZED,
            "TOKEN_REPLAY_DETECTED",
            err.to_string(),
        ),
        DataError::Calc(inner) => calc_error_response(inner),
        DataError::Sqlx(_) | DataError::InvalidDbData { .. } => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "DATA_ERROR",
            err.to_string(),
        ),
    }
}

fn calc_error_response(err: &CalceError) -> (StatusCode, &'static str, String) {
    match err {
        CalceError::CurrencyMismatch(_) => (
            StatusCode::BAD_REQUEST,
            "CURRENCY_MISMATCH",
            err.to_string(),
        ),
        CalceError::PriceNotFound { .. } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "PRICE_NOT_FOUND",
            err.to_string(),
        ),
        CalceError::FxRateNotFound { .. } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "FX_RATE_NOT_FOUND",
            err.to_string(),
        ),
        CalceError::InsufficientData { .. } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "INSUFFICIENT_DATA",
            err.to_string(),
        ),
        CalceError::CurrencyConflict { .. } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "CURRENCY_CONFLICT",
            err.to_string(),
        ),
    }
}
