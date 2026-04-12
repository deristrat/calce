use base64::Engine as _;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use ring::signature::KeyPair;
use serde::{Deserialize, Serialize};

use calce_core::domain::user::UserId;

use super::{JWT_AUDIENCE, JWT_ISSUER, Role, SecurityContext};

/// JWT access token lifetime in seconds (15 minutes).
pub const ACCESS_TOKEN_LIFETIME_SECS: u64 = 900;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Claims {
    pub sub: String,
    pub role: String,
    pub iss: String,
    pub aud: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org: Option<String>,
    pub iat: u64,
    pub exp: u64,
}

/// Mint a signed JWT access token.
///
/// # Errors
///
/// Returns an error if signing fails.
pub fn encode_access_token(
    user_id: &str,
    role: &Role,
    org: Option<&str>,
    encoding_key: &EncodingKey,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = now_secs();
    let claims = Claims {
        sub: user_id.to_owned(),
        role: match role {
            Role::Admin => "admin".to_owned(),
            Role::User => "user".to_owned(),
        },
        iss: JWT_ISSUER.to_owned(),
        aud: JWT_AUDIENCE.to_owned(),
        org: org.map(ToOwned::to_owned),
        iat: now,
        exp: now + ACCESS_TOKEN_LIFETIME_SECS,
    };
    encode(&Header::new(Algorithm::EdDSA), &claims, encoding_key)
}

/// Validate a JWT and extract a `SecurityContext`.
///
/// # Errors
///
/// Returns an error if the token is invalid, expired, or has a bad signature.
pub fn decode_access_token(
    token: &str,
    decoding_key: &DecodingKey,
) -> Result<SecurityContext, jsonwebtoken::errors::Error> {
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.set_required_spec_claims(&["sub", "exp", "iat"]);
    validation.set_issuer(&[JWT_ISSUER]);
    validation.set_audience(&[JWT_AUDIENCE]);

    let data = decode::<Claims>(token, decoding_key, &validation)?;
    let role = Role::parse(&data.claims.role);

    Ok(SecurityContext::new(UserId::new(&data.claims.sub), role))
}

/// Load Ed25519 keys from `CALCE_JWT_PRIVATE_KEY` env var.
///
/// # Panics
///
/// Panics if the env var is missing or contains invalid data.
#[allow(clippy::expect_used)] // startup — server cannot operate without valid keys
pub(crate) fn load_keys_from_env() -> (EncodingKey, DecodingKey) {
    let b64 = std::env::var("CALCE_JWT_PRIVATE_KEY").expect("CALCE_JWT_PRIVATE_KEY must be set");
    let pkcs8_der = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .expect("CALCE_JWT_PRIVATE_KEY must be valid base64");
    keys_from_pkcs8(&pkcs8_der)
}

/// Generate an ephemeral Ed25519 key pair (dev/test use only).
#[allow(clippy::expect_used)] // dev/test only — system RNG failure is unrecoverable
pub(crate) fn generate_ephemeral_keys() -> (EncodingKey, DecodingKey) {
    let rng = ring::rand::SystemRandom::new();
    let pkcs8_bytes = ring::signature::Ed25519KeyPair::generate_pkcs8(&rng)
        .expect("failed to generate Ed25519 key pair");
    keys_from_pkcs8(pkcs8_bytes.as_ref())
}

#[allow(clippy::expect_used)] // called from startup paths only
fn keys_from_pkcs8(pkcs8_der: &[u8]) -> (EncodingKey, DecodingKey) {
    let key_pair =
        ring::signature::Ed25519KeyPair::from_pkcs8(pkcs8_der).expect("invalid Ed25519 PKCS#8 DER");
    let encoding_key = EncodingKey::from_ed_der(pkcs8_der);
    // jsonwebtoken's from_ed_der expects raw 32-byte public key for EdDSA
    let decoding_key = DecodingKey::from_ed_der(key_pair.public_key().as_ref());
    (encoding_key, decoding_key)
}

#[allow(clippy::expect_used)] // system clock before unix epoch is unrecoverable
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_keys() -> (EncodingKey, DecodingKey) {
        generate_ephemeral_keys()
    }

    #[test]
    fn encode_decode_roundtrip() {
        let (enc, dec) = test_keys();
        let token = encode_access_token("alice", &Role::User, None, &enc).unwrap();
        let ctx = decode_access_token(&token, &dec).unwrap();
        assert_eq!(ctx.user_id, UserId::new("alice"));
        assert_eq!(ctx.role, Role::User);
    }

    #[test]
    fn admin_role_roundtrip() {
        let (enc, dec) = test_keys();
        let token = encode_access_token("bob", &Role::Admin, Some("org1"), &enc).unwrap();
        let ctx = decode_access_token(&token, &dec).unwrap();
        assert_eq!(ctx.user_id, UserId::new("bob"));
        assert_eq!(ctx.role, Role::Admin);
    }

    #[test]
    fn wrong_key_rejects() {
        let (enc, _) = test_keys();
        let (_, wrong_dec) = test_keys();
        let token = encode_access_token("alice", &Role::User, None, &enc).unwrap();
        assert!(decode_access_token(&token, &wrong_dec).is_err());
    }

    #[test]
    fn garbage_token_rejects() {
        let (_, dec) = test_keys();
        assert!(decode_access_token("not.a.jwt", &dec).is_err());
    }
}
