use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use calce_core::domain::user::UserId;
use calce_data::auth::SecurityContext;
use calce_data::auth::middleware;
use calce_data::error::DataError;
use serde_json::json;

use crate::error::ApiError;
use crate::state::AppState;

pub struct Auth(pub SecurityContext);

#[derive(Debug)]
pub enum AuthError {
    MissingToken,
    InvalidToken,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let message = match self {
            AuthError::MissingToken => "Missing Authorization: Bearer <token> header",
            AuthError::InvalidToken => "Invalid or expired token",
        };
        let body = json!({ "error": "UNAUTHORIZED", "message": message });
        (StatusCode::UNAUTHORIZED, axum::Json(body)).into_response()
    }
}

impl FromRequestParts<AppState> for Auth {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(auth_header) = parts.headers.get("authorization")
            && let Ok(value) = auth_header.to_str()
            && let Some(token) = value.strip_prefix("Bearer ")
        {
            let ctx = middleware::validate_bearer_token(
                token,
                &state.auth_config,
                state.pool.as_ref(),
                Some(&state.api_key_cache),
            )
            .await
            .map_err(|_| AuthError::InvalidToken)?;
            return Ok(Auth(ctx));
        }

        Err(AuthError::MissingToken)
    }
}

/// Require unrestricted admin (human user, not org-scoped API key).
pub fn require_admin(ctx: &SecurityContext) -> Result<(), ApiError> {
    if ctx.is_unrestricted_admin() {
        Ok(())
    } else {
        Err(ApiError::Data(DataError::Unauthorized {
            requester: ctx.user_id.clone(),
            target: UserId::new("*"),
        }))
    }
}

/// Require admin with access to a specific organization.
/// Human admins pass unconditionally; org-scoped admins (API keys)
/// must belong to the requested org.
pub fn require_org_admin(ctx: &SecurityContext, target_org: &str) -> Result<(), ApiError> {
    if !ctx.is_admin() {
        return Err(ApiError::Data(DataError::Unauthorized {
            requester: ctx.user_id.clone(),
            target: UserId::new(target_org),
        }));
    }
    // Org-scoped admin: must match the target org
    if let Some(ref org_id) = ctx.org_id
        && org_id != target_org
    {
        return Err(ApiError::Data(DataError::Unauthorized {
            requester: ctx.user_id.clone(),
            target: UserId::new(target_org),
        }));
    }
    Ok(())
}

/// Extract and validate a JWT from either the `Authorization: Bearer` header
/// or a `?token=` query parameter (needed for EventSource which can't set headers).
/// Requires admin role.
pub async fn require_admin_from_sse(
    headers: &axum::http::HeaderMap,
    query_token: Option<String>,
    state: &AppState,
) -> Result<(), ApiError> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(String::from)
        .or(query_token)
        .ok_or(ApiError::Data(DataError::InvalidCredentials))?;

    let ctx = middleware::validate_bearer_token(
        &token,
        &state.auth_config,
        state.pool.as_ref(),
        Some(&state.api_key_cache),
    )
    .await
    .map_err(|_| ApiError::Data(DataError::InvalidCredentials))?;
    require_admin(&ctx)
}

pub fn require_access(ctx: &SecurityContext, target: &str) -> Result<(), ApiError> {
    let target_id = UserId::new(target);
    if ctx.can_access(&target_id) {
        Ok(())
    } else {
        Err(ApiError::Data(DataError::Unauthorized {
            requester: ctx.user_id.clone(),
            target: target_id,
        }))
    }
}
