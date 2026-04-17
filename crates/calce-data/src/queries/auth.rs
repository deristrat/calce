use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{DataError, DataResult};

use serde::Serialize;

// -- Row types ---------------------------------------------------------------

#[derive(Debug)]
pub struct CredentialRow {
    pub credential_id: i64,
    pub user_internal_id: i64,
    pub user_external_id: String,
    pub email: String,
    pub role: String,
    pub password_hash: String,
    pub failed_attempts: i32,
    pub locked_until: Option<DateTime<Utc>>,
}

#[derive(Debug)]
pub struct RefreshTokenRow {
    pub id: i64,
    pub user_id: i64,
    pub user_external_id: String,
    pub user_role: String,
    pub family_id: Uuid,
    pub token_hash: String,
    pub superseded_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug)]
#[allow(dead_code)] // fields populated by query_as! but read selectively by callers
pub(crate) struct ApiKeyRow {
    pub id: i64,
    pub organization_id: i64,
    pub organization_external_id: String,
    pub name: String,
    pub key_prefix: String,
    pub key_hash: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ApiKeyListRow {
    pub id: i64,
    pub name: String,
    pub key_prefix: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

// -- Credential queries ------------------------------------------------------

pub struct AuthRepo;

impl AuthRepo {
    /// Look up a user's credentials by email.
    pub async fn find_credential_by_email(
        pool: &PgPool,
        email: &str,
    ) -> DataResult<Option<CredentialRow>> {
        let row = sqlx::query_as!(
            CredentialRow,
            r#"SELECT uc.id AS credential_id,
                    u.id AS user_internal_id,
                    u.external_id AS user_external_id,
                    u.email AS "email!",
                    u.role,
                    uc.password_hash,
                    uc.failed_attempts,
                    uc.locked_until
             FROM user_credentials uc
             JOIN users u ON uc.user_id = u.id
             WHERE u.email = $1"#,
            email,
        )
        .fetch_optional(pool)
        .await?;
        Ok(row)
    }

    pub async fn increment_failed_attempts(pool: &PgPool, credential_id: i64) -> DataResult<i32> {
        let row = sqlx::query_scalar!(
            "UPDATE user_credentials \
             SET failed_attempts = failed_attempts + 1 \
             WHERE id = $1 \
             RETURNING failed_attempts",
            credential_id,
        )
        .fetch_one(pool)
        .await?;
        Ok(row)
    }

    pub async fn lock_account(
        pool: &PgPool,
        credential_id: i64,
        until: DateTime<Utc>,
    ) -> DataResult<()> {
        sqlx::query!(
            "UPDATE user_credentials \
             SET failed_attempts = failed_attempts + 1, locked_until = $2 \
             WHERE id = $1",
            credential_id,
            until,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn reset_failed_attempts(pool: &PgPool, credential_id: i64) -> DataResult<()> {
        sqlx::query!(
            "UPDATE user_credentials \
             SET failed_attempts = 0, locked_until = NULL \
             WHERE id = $1",
            credential_id,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Create a password credential for a user.
    pub async fn create_credential(
        pool: &PgPool,
        user_id: i64,
        password_hash: &str,
    ) -> DataResult<i64> {
        let id = sqlx::query_scalar!(
            "INSERT INTO user_credentials (user_id, password_hash) \
             VALUES ($1, $2) \
             RETURNING id",
            user_id,
            password_hash,
        )
        .fetch_one(pool)
        .await
        .map_err(|e| DataError::from_constraint_violation(e, "credential", &user_id.to_string()))?;
        Ok(id)
    }

    // -- Refresh token queries -----------------------------------------------

    pub async fn create_refresh_token(
        pool: &PgPool,
        user_id: i64,
        family_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> DataResult<i64> {
        let id = sqlx::query_scalar!(
            "INSERT INTO refresh_tokens (user_id, family_id, token_hash, expires_at) \
             VALUES ($1, $2, $3, $4) \
             RETURNING id",
            user_id,
            family_id,
            token_hash,
            expires_at,
        )
        .fetch_one(pool)
        .await?;
        Ok(id)
    }

    /// Look up a refresh token by its HMAC hash, joining user info.
    pub async fn find_refresh_token(
        pool: &PgPool,
        token_hash: &str,
    ) -> DataResult<Option<RefreshTokenRow>> {
        let row = sqlx::query_as!(
            RefreshTokenRow,
            "SELECT rt.id, rt.user_id, \
                    u.external_id AS user_external_id, \
                    u.role AS user_role, \
                    rt.family_id, rt.token_hash, \
                    rt.superseded_at, rt.revoked_at, \
                    rt.expires_at, rt.created_at \
             FROM refresh_tokens rt \
             JOIN users u ON rt.user_id = u.id \
             WHERE rt.token_hash = $1",
            token_hash,
        )
        .fetch_optional(pool)
        .await?;
        Ok(row)
    }

    /// Find the currently active token in a family (for grace-period handling).
    pub async fn find_active_family_token(
        pool: &PgPool,
        family_id: Uuid,
    ) -> DataResult<Option<RefreshTokenRow>> {
        let row = sqlx::query_as!(
            RefreshTokenRow,
            "SELECT rt.id, rt.user_id, \
                    u.external_id AS user_external_id, \
                    u.role AS user_role, \
                    rt.family_id, rt.token_hash, \
                    rt.superseded_at, rt.revoked_at, \
                    rt.expires_at, rt.created_at \
             FROM refresh_tokens rt \
             JOIN users u ON rt.user_id = u.id \
             WHERE rt.family_id = $1 \
               AND rt.superseded_at IS NULL \
               AND rt.revoked_at IS NULL \
               AND rt.expires_at > NOW() \
             ORDER BY rt.created_at DESC \
             LIMIT 1",
            family_id,
        )
        .fetch_optional(pool)
        .await?;
        Ok(row)
    }

    /// Mark a refresh token as superseded (rotated out).
    pub async fn supersede_refresh_token(pool: &PgPool, token_id: i64) -> DataResult<()> {
        sqlx::query!(
            "UPDATE refresh_tokens SET superseded_at = NOW() WHERE id = $1",
            token_id,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Revoke all tokens in a family (replay attack detected).
    pub async fn revoke_token_family(pool: &PgPool, family_id: Uuid) -> DataResult<()> {
        sqlx::query!(
            "UPDATE refresh_tokens SET revoked_at = NOW() \
             WHERE family_id = $1 AND revoked_at IS NULL",
            family_id,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    // -- API key queries -----------------------------------------------------

    pub async fn create_api_key(
        pool: &PgPool,
        organization_id: i64,
        name: &str,
        key_prefix: &str,
        key_hash: &str,
        expires_at: Option<DateTime<Utc>>,
    ) -> DataResult<i64> {
        let id = sqlx::query_scalar!(
            "INSERT INTO api_keys (organization_id, name, key_prefix, key_hash, expires_at) \
             VALUES ($1, $2, $3, $4, $5) \
             RETURNING id",
            organization_id,
            name,
            key_prefix,
            key_hash,
            expires_at,
        )
        .fetch_one(pool)
        .await?;
        Ok(id)
    }

    /// Look up an API key by its HMAC hash, joining organization info.
    pub(crate) async fn find_api_key_by_hash(
        pool: &PgPool,
        key_hash: &str,
    ) -> DataResult<Option<ApiKeyRow>> {
        let row = sqlx::query_as!(
            ApiKeyRow,
            "SELECT ak.id, ak.organization_id, \
                    o.external_id AS organization_external_id, \
                    ak.name, ak.key_prefix, ak.key_hash, \
                    ak.expires_at, ak.revoked_at, ak.created_at \
             FROM api_keys ak \
             JOIN organizations o ON ak.organization_id = o.id \
             WHERE ak.key_hash = $1",
            key_hash,
        )
        .fetch_optional(pool)
        .await?;
        Ok(row)
    }

    /// List active (non-revoked) API keys for an organization.
    pub async fn list_api_keys(
        pool: &PgPool,
        org_external_id: &str,
    ) -> DataResult<Vec<ApiKeyListRow>> {
        let rows = sqlx::query_as!(
            ApiKeyListRow,
            "SELECT ak.id, ak.name, ak.key_prefix, \
                    ak.expires_at, ak.revoked_at, ak.created_at \
             FROM api_keys ak \
             JOIN organizations o ON ak.organization_id = o.id \
             WHERE o.external_id = $1 \
               AND ak.revoked_at IS NULL \
             ORDER BY ak.created_at DESC",
            org_external_id,
        )
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }

    /// Revoke an API key (soft delete). Returns the `key_hash` for cache eviction.
    pub async fn revoke_api_key(
        pool: &PgPool,
        key_id: i64,
        org_external_id: &str,
    ) -> DataResult<Option<String>> {
        let hash = sqlx::query_scalar!(
            "UPDATE api_keys SET revoked_at = NOW() \
             FROM organizations o \
             WHERE api_keys.organization_id = o.id \
               AND o.external_id = $2 \
               AND api_keys.id = $1 \
               AND api_keys.revoked_at IS NULL \
             RETURNING api_keys.key_hash",
            key_id,
            org_external_id,
        )
        .fetch_optional(pool)
        .await?;
        Ok(hash)
    }

    /// Revoke a token family by refresh token hash (for logout).
    /// Returns the `family_id` if found, `None` if the token doesn't exist.
    pub async fn revoke_family_by_token_hash(
        pool: &PgPool,
        token_hash: &str,
    ) -> DataResult<Option<Uuid>> {
        let family_id = sqlx::query_scalar!(
            "SELECT family_id FROM refresh_tokens WHERE token_hash = $1",
            token_hash,
        )
        .fetch_optional(pool)
        .await?;

        if let Some(fid) = family_id {
            Self::revoke_token_family(pool, fid).await?;
        }
        Ok(family_id)
    }

    /// Look up internal org ID by external ID.
    pub async fn get_org_internal_id(pool: &PgPool, external_id: &str) -> DataResult<Option<i64>> {
        let id = sqlx::query_scalar!(
            "SELECT id FROM organizations WHERE external_id = $1",
            external_id,
        )
        .fetch_optional(pool)
        .await?;
        Ok(id)
    }
}
