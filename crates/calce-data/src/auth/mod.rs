pub mod api_key;
pub mod jwt;
pub mod middleware;
pub mod password;
pub mod tokens;

use calce_core::domain::user::UserId;

use crate::permissions;

pub(crate) const JWT_ISSUER: &str = "calce";
pub(crate) const JWT_AUDIENCE: &str = "calce-api";

// Auth policy constants — shared by API and Python bindings.
pub const LOCKOUT_THRESHOLD: i32 = 10;
pub const LOCKOUT_DURATION_MINUTES: i64 = 15;
pub const REFRESH_TOKEN_LIFETIME_DAYS: i64 = 30;
/// Grace period for refresh token reuse (seconds). Within this window,
/// replaying an old token returns the same new token instead of revoking.
pub const GRACE_PERIOD_SECS: i64 = 30;
pub const MAX_PASSWORD_LENGTH: usize = 128;

/// Dummy Argon2 hash for timing-safe password verification when the user
/// doesn't exist (prevents user-enumeration via response time).
pub const DUMMY_PASSWORD_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$AAAAAAAAAAAAAAAAAAAAgA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Role {
    Admin,
    User,
}

impl Role {
    #[must_use]
    pub fn parse(s: &str) -> Self {
        if s.eq_ignore_ascii_case("admin") {
            Role::Admin
        } else {
            Role::User
        }
    }
}

#[derive(Clone, Debug)]
pub struct SecurityContext {
    pub user_id: UserId,
    pub role: Role,
    /// Set for org-scoped service accounts (API keys). `None` for human users.
    pub org_id: Option<String>,
}

impl SecurityContext {
    #[must_use]
    pub fn new(user_id: UserId, role: Role) -> Self {
        SecurityContext {
            user_id,
            role,
            org_id: None,
        }
    }

    #[must_use]
    pub fn with_org(mut self, org_id: String) -> Self {
        self.org_id = Some(org_id);
        self
    }

    #[must_use]
    pub fn system() -> Self {
        SecurityContext {
            user_id: UserId::new("system"),
            role: Role::Admin,
            org_id: None,
        }
    }

    /// Delegates to [`permissions::can_access_user_data`].
    #[must_use]
    pub fn can_access(&self, target: &UserId) -> bool {
        permissions::can_access_user_data(self, target)
    }

    #[must_use]
    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }

    /// True if this is a human admin (not an org-scoped service account).
    #[must_use]
    pub fn is_unrestricted_admin(&self) -> bool {
        self.role == Role::Admin && self.org_id.is_none()
    }
}

/// Auth system configuration.
#[derive(Clone)]
pub struct AuthConfig {
    pub jwt_encoding_key: jsonwebtoken::EncodingKey,
    pub jwt_decoding_key: jsonwebtoken::DecodingKey,
    pub hmac_secret: Vec<u8>,
}

impl AuthConfig {
    /// Create config from environment variables.
    ///
    /// Reads:
    /// - `CALCE_JWT_PRIVATE_KEY` — base64 Ed25519 PKCS#8 DER
    /// - `CALCE_HMAC_SECRET` — token hashing key
    ///
    /// # Panics
    ///
    /// Panics if either variable is missing or invalid. Use a `.env` file
    /// in the project root for local development (loaded via `dotenvy`).
    pub fn from_env() -> Self {
        let (encoding_key, decoding_key) = jwt::load_keys_from_env();

        let hmac_secret = std::env::var("CALCE_HMAC_SECRET")
            .expect("CALCE_HMAC_SECRET must be set")
            .into_bytes();

        AuthConfig {
            jwt_encoding_key: encoding_key,
            jwt_decoding_key: decoding_key,
            hmac_secret,
        }
    }

    /// Config with ephemeral keys for tests.
    #[must_use]
    pub fn test_default() -> Self {
        let (encoding_key, decoding_key) = jwt::generate_ephemeral_keys();
        AuthConfig {
            jwt_encoding_key: encoding_key,
            jwt_decoding_key: decoding_key,
            hmac_secret: b"test-hmac-secret".to_vec(),
        }
    }
}
