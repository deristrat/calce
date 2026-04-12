use base64::Engine as _;
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Generate a cryptographically secure random token, base64url-encoded (43 chars).
#[must_use]
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// HMAC-SHA256 hash of a token value, returned as a hex string.
///
/// Uses a server-side secret so that a DB leak alone is insufficient
/// to verify tokens offline.
#[must_use]
#[allow(clippy::expect_used)] // HMAC-SHA256 accepts any key length
pub fn hmac_hash(token: &str, secret: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(token.as_bytes());
    hex_encode(&mac.finalize().into_bytes())
}

#[allow(clippy::unwrap_used)] // write! to String is infallible
fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(s, "{b:02x}").unwrap();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_are_unique() {
        let t1 = generate_token();
        let t2 = generate_token();
        assert_ne!(t1, t2);
        assert_eq!(t1.len(), 43); // 32 bytes base64url-encoded without padding
    }

    #[test]
    fn hmac_is_deterministic() {
        let h1 = hmac_hash("my-token", b"secret");
        let h2 = hmac_hash("my-token", b"secret");
        assert_eq!(h1, h2);
    }

    #[test]
    fn hmac_differs_for_different_tokens() {
        let h1 = hmac_hash("token-a", b"secret");
        let h2 = hmac_hash("token-b", b"secret");
        assert_ne!(h1, h2);
    }

    #[test]
    fn hmac_differs_for_different_secrets() {
        let h1 = hmac_hash("same-token", b"secret-1");
        let h2 = hmac_hash("same-token", b"secret-2");
        assert_ne!(h1, h2);
    }
}
