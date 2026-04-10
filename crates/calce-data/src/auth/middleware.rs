use super::api_key::{self, ApiKeyCache, CachedApiKey};
use super::jwt;
use super::tokens;
use super::{AuthConfig, SecurityContext};
use crate::queries::auth::AuthRepo;

/// Resolve a bearer token to a `SecurityContext`.
///
/// Tries JWT decode first (no DB hit). Falls through to API key lookup
/// with in-memory cache.
pub async fn validate_bearer_token(
    token: &str,
    config: &AuthConfig,
    pool: Option<&sqlx::PgPool>,
    cache: Option<&ApiKeyCache>,
) -> Result<SecurityContext, AuthValidationError> {
    // 1. Try JWT (no DB hit)
    if let Ok(ctx) = jwt::decode_access_token(token, &config.jwt_decoding_key) {
        return Ok(ctx);
    }

    // 2. Try API key (cache → DB)
    if let Some(pool) = pool
        && let Some(cache) = cache
    {
        let key_hash = tokens::hmac_hash(token, &config.hmac_secret);

        // Check cache first
        if let Some(cached) = cache.get(&key_hash).await {
            return api_key::validate_cached_key(&cached).ok_or(AuthValidationError::InvalidToken);
        }

        // Cache miss — look up in DB
        if let Ok(Some(row)) = AuthRepo::find_api_key_by_hash(pool, &key_hash).await {
            let entry = CachedApiKey {
                organization_external_id: row.organization_external_id.clone(),
                expires_at: row.expires_at,
                revoked_at: row.revoked_at,
            };
            cache.insert(key_hash, entry.clone()).await;
            return api_key::validate_cached_key(&entry).ok_or(AuthValidationError::InvalidToken);
        }
    }

    Err(AuthValidationError::InvalidToken)
}

#[derive(Debug)]
pub enum AuthValidationError {
    InvalidToken,
}
