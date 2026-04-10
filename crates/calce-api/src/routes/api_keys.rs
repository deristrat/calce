use axum::extract::{Path, State};
use axum::routing::{delete, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use calce_data::auth::api_key;
use calce_data::error::DataError;
use calce_data::queries::auth::{ApiKeyListRow, AuthRepo};

use crate::auth::{self, Auth};
use crate::error::ApiError;
use crate::state::AppState;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/v1/organizations/{org_id}/api-keys",
            post(create_api_key).get(list_api_keys),
        )
        .route(
            "/v1/organizations/{org_id}/api-keys/{key_id}",
            delete(revoke_api_key),
        )
}

#[derive(Deserialize)]
struct CreateApiKeyRequest {
    name: String,
    #[serde(default)]
    environment: Option<String>,
    #[serde(default)]
    expires_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
struct CreateApiKeyResponse {
    id: i64,
    name: String,
    key: String,
    key_prefix: String,
    expires_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
struct ApiKeyListResponse {
    items: Vec<ApiKeyListRow>,
}

async fn create_api_key(
    Auth(ctx): Auth,
    State(state): State<AppState>,
    Path(org_id): Path<String>,
    Json(body): Json<CreateApiKeyRequest>,
) -> Result<Json<CreateApiKeyResponse>, ApiError> {
    auth::require_org_admin(&ctx, &org_id)?;

    let pool = state.require_pool()?;

    let org_internal_id = AuthRepo::get_org_internal_id(pool, &org_id)
        .await?
        .ok_or_else(|| DataError::NotFound(format!("organization '{org_id}'")))?;

    let env = body.environment.as_deref().unwrap_or("live");
    let (full_key, prefix, key_hash) =
        api_key::generate_api_key(env, &state.auth_config.hmac_secret);

    let id = AuthRepo::create_api_key(
        pool,
        org_internal_id,
        &body.name,
        &prefix,
        &key_hash,
        body.expires_at,
    )
    .await?;

    Ok(Json(CreateApiKeyResponse {
        id,
        name: body.name,
        key: full_key,
        key_prefix: prefix,
        expires_at: body.expires_at,
    }))
}

async fn list_api_keys(
    Auth(ctx): Auth,
    State(state): State<AppState>,
    Path(org_id): Path<String>,
) -> Result<Json<ApiKeyListResponse>, ApiError> {
    auth::require_org_admin(&ctx, &org_id)?;

    let pool = state.require_pool()?;
    let items = AuthRepo::list_api_keys(pool, &org_id).await?;
    Ok(Json(ApiKeyListResponse { items }))
}

async fn revoke_api_key(
    Auth(ctx): Auth,
    State(state): State<AppState>,
    Path((org_id, key_id)): Path<(String, i64)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    auth::require_org_admin(&ctx, &org_id)?;

    let pool = state.require_pool()?;

    let key_hash = AuthRepo::revoke_api_key(pool, key_id, &org_id)
        .await?
        .ok_or_else(|| DataError::NotFound(format!("api key {key_id}")))?;

    // Best-effort cache eviction
    state.api_key_cache.evict(&key_hash).await;

    Ok(Json(serde_json::json!({ "revoked": true })))
}
