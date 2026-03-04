use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use calce_core::error::CalceError;
use serde_json::json;

pub enum ApiError {
    Calce(CalceError),
    BadRequest(String),
}

impl From<CalceError> for ApiError {
    fn from(err: CalceError) -> Self {
        ApiError::Calce(err)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg),
            ApiError::Calce(ref err) => match err {
                CalceError::Unauthorized { .. } => {
                    (StatusCode::FORBIDDEN, "UNAUTHORIZED", err.to_string())
                }
                CalceError::NoTradesFound(_) => {
                    (StatusCode::NOT_FOUND, "NO_TRADES_FOUND", err.to_string())
                }
                CalceError::CurrencyMismatch(_) => {
                    (StatusCode::BAD_REQUEST, "CURRENCY_MISMATCH", err.to_string())
                }
                CalceError::PriceNotFound { .. } => {
                    (StatusCode::INTERNAL_SERVER_ERROR, "PRICE_NOT_FOUND", err.to_string())
                }
                CalceError::FxRateNotFound { .. } => {
                    (StatusCode::INTERNAL_SERVER_ERROR, "FX_RATE_NOT_FOUND", err.to_string())
                }
                CalceError::InsufficientData { .. } => {
                    (StatusCode::UNPROCESSABLE_ENTITY, "INSUFFICIENT_DATA", err.to_string())
                }
            },
        };

        let body = json!({ "error": code, "message": message });
        (status, axum::Json(body)).into_response()
    }
}
