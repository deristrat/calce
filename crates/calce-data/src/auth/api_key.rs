use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use moka::future::Cache;

use super::tokens;
use super::{Role, SecurityContext};
use calce_core::domain::user::UserId;

/// Structured prefix for API keys, enabling secret scanning.
const LIVE_PREFIX: &str = "calce_live_";
const TEST_PREFIX: &str = "calce_test_";

/// Cache TTL — revocations propagate within this window.
const CACHE_TTL: Duration = Duration::from_secs(60);

/// Cached result of an API key lookup.
#[derive(Clone, Debug)]
pub(crate) struct CachedApiKey {
    pub organization_external_id: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

/// Thread-safe API key cache with TTL eviction.
#[derive(Clone)]
pub struct ApiKeyCache {
    inner: Cache<String, Arc<CachedApiKey>>,
}

impl ApiKeyCache {
    #[must_use]
    pub fn new() -> Self {
        let cache = Cache::builder()
            .time_to_live(CACHE_TTL)
            .max_capacity(10_000)
            .build();
        ApiKeyCache { inner: cache }
    }

    pub(crate) async fn get(&self, key_hash: &str) -> Option<Arc<CachedApiKey>> {
        self.inner.get(key_hash).await
    }

    pub(crate) async fn insert(&self, key_hash: String, entry: CachedApiKey) {
        self.inner.insert(key_hash, Arc::new(entry)).await;
    }

    pub async fn evict(&self, key_hash: &str) {
        self.inner.remove(key_hash).await;
    }
}

impl Default for ApiKeyCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a new API key with a structured prefix.
///
/// Returns `(full_key, prefix, hmac_hash)`. The full key is returned to the
/// caller exactly once; only the hash is stored.
#[must_use]
pub fn generate_api_key(environment: &str, hmac_secret: &[u8]) -> (String, String, String) {
    let prefix = match environment {
        "test" => TEST_PREFIX,
        _ => LIVE_PREFIX,
    };
    let random_part = tokens::generate_token();
    let full_key = format!("{prefix}{random_part}");
    let key_hash = tokens::hmac_hash(&full_key, hmac_secret);
    let stored_prefix = format!("{prefix}{}", &random_part[..8]);
    (full_key, stored_prefix, key_hash)
}

/// Validate a cached API key entry and build a `SecurityContext`.
///
/// API keys get admin role scoped to their organization via `org_id`.
/// The permissions layer denies cross-org access for org-scoped admins.
pub(crate) fn validate_cached_key(cached: &CachedApiKey) -> Option<SecurityContext> {
    if cached.revoked_at.is_some() {
        return None;
    }
    if let Some(expires_at) = cached.expires_at
        && expires_at < Utc::now()
    {
        return None;
    }
    Some(
        SecurityContext::new(UserId::new(&cached.organization_external_id), Role::Admin)
            .with_org(cached.organization_external_id.clone()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_key_has_live_prefix() {
        let (key, prefix, hash) = generate_api_key("live", b"secret");
        assert!(key.starts_with("calce_live_"));
        assert!(prefix.starts_with("calce_live_"));
        assert_eq!(prefix.len(), "calce_live_".len() + 8);
        assert!(!hash.is_empty());
    }

    #[test]
    fn generate_key_has_test_prefix() {
        let (key, prefix, _) = generate_api_key("test", b"secret");
        assert!(key.starts_with("calce_test_"));
        assert!(prefix.starts_with("calce_test_"));
        assert_eq!(prefix.len(), "calce_test_".len() + 8);
    }

    #[test]
    fn generated_keys_are_unique() {
        let (k1, _, _) = generate_api_key("live", b"secret");
        let (k2, _, _) = generate_api_key("live", b"secret");
        assert_ne!(k1, k2);
    }

    #[test]
    fn validate_rejects_revoked() {
        let cached = CachedApiKey {
            organization_external_id: "org1".into(),
            expires_at: None,
            revoked_at: Some(Utc::now()),
        };
        assert!(validate_cached_key(&cached).is_none());
    }

    #[test]
    fn validate_rejects_expired() {
        let cached = CachedApiKey {
            organization_external_id: "org1".into(),
            expires_at: Some(Utc::now() - chrono::Duration::hours(1)),
            revoked_at: None,
        };
        assert!(validate_cached_key(&cached).is_none());
    }

    #[test]
    fn validate_accepts_valid_key() {
        let cached = CachedApiKey {
            organization_external_id: "org1".into(),
            expires_at: None,
            revoked_at: None,
        };
        let ctx = validate_cached_key(&cached).unwrap();
        assert_eq!(ctx.user_id, UserId::new("org1"));
        assert_eq!(ctx.role, Role::Admin);
    }

    #[tokio::test]
    async fn cache_insert_and_get() {
        let cache = ApiKeyCache::new();
        let entry = CachedApiKey {
            organization_external_id: "org1".into(),
            expires_at: None,
            revoked_at: None,
        };
        cache.insert("hash123".into(), entry).await;
        let got = cache.get("hash123").await;
        assert!(got.is_some());
    }

    #[tokio::test]
    async fn cache_evict() {
        let cache = ApiKeyCache::new();
        let entry = CachedApiKey {
            organization_external_id: "org1".into(),
            expires_at: None,
            revoked_at: None,
        };
        cache.insert("hash456".into(), entry).await;
        cache.evict("hash456").await;
        assert!(cache.get("hash456").await.is_none());
    }
}
