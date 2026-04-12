use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use calce_data::auth::{
    DUMMY_PASSWORD_HASH, GRACE_PERIOD_SECS, LOCKOUT_DURATION_MINUTES, LOCKOUT_THRESHOLD,
    MAX_PASSWORD_LENGTH, REFRESH_TOKEN_LIFETIME_DAYS, Role, jwt, password, tokens,
};
use calce_data::error::DataError;
use calce_data::queries::auth::{AuthRepo, CredentialRow};

use crate::error::ApiError;
use crate::rate_limit;
use crate::state::AppState;

#[derive(Deserialize)]
pub(crate) struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub(crate) struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Deserialize)]
pub(crate) struct LogoutRequest {
    pub refresh_token: String,
}

#[derive(Serialize)]
pub(crate) struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: &'static str,
    pub expires_in: u64,
}

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/refresh", post(refresh))
        .route("/auth/logout", post(logout))
}

// -- Helpers -----------------------------------------------------------------

/// Extract client IP and check rate limit for auth endpoints.
fn check_auth_rate_limit(headers: &HeaderMap, state: &AppState) -> Result<(), ApiError> {
    let xff = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok());
    let ip = rate_limit::extract_ip(xff);
    rate_limit::check_rate_limit(&state.auth_rate_limiter, ip)
}

/// Basic email format check — must contain exactly one '@' with non-empty local and domain parts.
fn validate_email(email: &str) -> Result<(), ApiError> {
    let at_count = email.chars().filter(|&c| c == '@').count();
    if at_count != 1 {
        return Err(ApiError::BadRequest("Invalid email address".into()));
    }
    #[allow(clippy::unwrap_used)] // at_count == 1 guarantees split_once succeeds
    let (local, domain) = email.split_once('@').unwrap();
    if local.is_empty() || domain.is_empty() || !domain.contains('.') {
        return Err(ApiError::BadRequest("Invalid email address".into()));
    }
    Ok(())
}

/// Mint access + refresh tokens and persist the refresh token.
async fn issue_tokens(
    pool: &sqlx::PgPool,
    config: &calce_data::auth::AuthConfig,
    user_internal_id: i64,
    user_external_id: &str,
    role: &Role,
    family_id: Uuid,
) -> Result<TokenResponse, ApiError> {
    let access_token =
        jwt::encode_access_token(user_external_id, role, None, &config.jwt_encoding_key)
            .map_err(|e| ApiError::BadRequest(format!("token error: {e}")))?;

    let refresh_value = tokens::generate_token();
    let token_hash = tokens::hmac_hash(&refresh_value, &config.hmac_secret);
    let expires_at = Utc::now() + Duration::days(REFRESH_TOKEN_LIFETIME_DAYS);

    AuthRepo::create_refresh_token(pool, user_internal_id, family_id, &token_hash, expires_at)
        .await?;

    Ok(TokenResponse {
        access_token,
        refresh_token: refresh_value,
        token_type: "Bearer",
        expires_in: jwt::ACCESS_TOKEN_LIFETIME_SECS,
    })
}

// -- Handlers ----------------------------------------------------------------

async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<LoginRequest>,
) -> Result<Json<TokenResponse>, ApiError> {
    check_auth_rate_limit(&headers, &state)?;
    validate_email(&body.email)?;

    // Cap password length to prevent Argon2 resource exhaustion
    if body.password.len() > MAX_PASSWORD_LENGTH {
        return Err(DataError::InvalidCredentials.into());
    }

    let pool = state.require_pool()?;
    let config = &state.auth_config;

    let cred = AuthRepo::find_credential_by_email(pool, &body.email).await?;

    // Timing-safe: always run a password hash comparison even when the user
    // doesn't exist, so the response time doesn't reveal valid emails.
    let (cred, password_ok) = if let Some(c) = cred {
        let ok = password::verify_password(&body.password, &c.password_hash).is_ok();
        (Some(c), ok)
    } else {
        let _ = password::verify_password(&body.password, DUMMY_PASSWORD_HASH);
        (None, false)
    };

    let cred = match (cred, password_ok) {
        (Some(c), true) => c,
        (Some(c), false) => {
            record_failed_attempt(pool, &c).await?;
            return Err(DataError::InvalidCredentials.into());
        }
        _ => return Err(DataError::InvalidCredentials.into()),
    };

    // Check lockout (after password check to keep timing constant)
    if let Some(locked_until) = cred.locked_until
        && locked_until > Utc::now()
    {
        return Err(DataError::AccountLocked {
            retry_after: locked_until,
        }
        .into());
    }

    if cred.failed_attempts > 0 {
        AuthRepo::reset_failed_attempts(pool, cred.credential_id).await?;
    }

    let role = Role::parse(&cred.role);
    let family_id = Uuid::new_v4();
    let response = issue_tokens(
        pool,
        config,
        cred.user_internal_id,
        &cred.user_external_id,
        &role,
        family_id,
    )
    .await?;

    Ok(Json(response))
}

async fn record_failed_attempt(pool: &sqlx::PgPool, cred: &CredentialRow) -> Result<(), ApiError> {
    let new_count = cred.failed_attempts + 1;
    if new_count >= LOCKOUT_THRESHOLD {
        let until = Utc::now() + Duration::minutes(LOCKOUT_DURATION_MINUTES);
        AuthRepo::lock_account(pool, cred.credential_id, until).await?;
    } else {
        AuthRepo::increment_failed_attempts(pool, cred.credential_id).await?;
    }
    Ok(())
}

async fn refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RefreshRequest>,
) -> Result<Json<TokenResponse>, ApiError> {
    check_auth_rate_limit(&headers, &state)?;

    let pool = state.require_pool()?;
    let config = &state.auth_config;

    let token_hash = tokens::hmac_hash(&body.refresh_token, &config.hmac_secret);

    let row = AuthRepo::find_refresh_token(pool, &token_hash)
        .await?
        .ok_or(DataError::InvalidRefreshToken)?;

    if row.revoked_at.is_some() {
        return Err(DataError::InvalidRefreshToken.into());
    }

    if row.expires_at < Utc::now() {
        return Err(DataError::InvalidRefreshToken.into());
    }

    // Already superseded — check grace period
    if let Some(superseded_at) = row.superseded_at {
        let elapsed = Utc::now() - superseded_at;
        if elapsed.num_seconds() < GRACE_PERIOD_SECS {
            // Within grace period: return a new access token but reuse the
            // already-rotated refresh token (don't mint yet another one).
            let role = Role::parse(&row.user_role);
            let access_token = jwt::encode_access_token(
                &row.user_external_id,
                &role,
                None,
                &config.jwt_encoding_key,
            )
            .map_err(|e| ApiError::BadRequest(format!("token error: {e}")))?;

            // Verify the family still has an active token
            let _active = AuthRepo::find_active_family_token(pool, row.family_id)
                .await?
                .ok_or(DataError::InvalidRefreshToken)?;

            return Ok(Json(TokenResponse {
                access_token,
                // Client should adopt this (the already-rotated) refresh token
                refresh_token: body.refresh_token,
                token_type: "Bearer",
                expires_in: jwt::ACCESS_TOKEN_LIFETIME_SECS,
            }));
        }
        // Outside grace period — replay attack, revoke entire family
        AuthRepo::revoke_token_family(pool, row.family_id).await?;
        return Err(DataError::TokenReplayDetected.into());
    }

    // Active token — rotate
    AuthRepo::supersede_refresh_token(pool, row.id).await?;

    let role = Role::parse(&row.user_role);
    let response = issue_tokens(
        pool,
        config,
        row.user_id,
        &row.user_external_id,
        &role,
        row.family_id,
    )
    .await?;

    Ok(Json(response))
}

async fn logout(
    State(state): State<AppState>,
    Json(body): Json<LogoutRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.require_pool()?;
    let config = &state.auth_config;

    let token_hash = tokens::hmac_hash(&body.refresh_token, &config.hmac_secret);
    AuthRepo::revoke_family_by_token_hash(pool, &token_hash).await?;

    Ok(Json(serde_json::json!({ "logged_out": true })))
}
