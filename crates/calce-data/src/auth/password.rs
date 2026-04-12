use argon2::password_hash::SaltString;
use argon2::{Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version};
use rand::rngs::OsRng;

/// OWASP recommended minimums for Argon2id (2024).
const MEMORY_COST_KIB: u32 = 19_456; // 19 MiB
const TIME_COST: u32 = 2;
const PARALLELISM: u32 = 1;

#[allow(clippy::expect_used)] // compile-time-known valid params
fn argon2_instance() -> Argon2<'static> {
    let params =
        Params::new(MEMORY_COST_KIB, TIME_COST, PARALLELISM, None).expect("valid argon2 params");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// Hash a password using Argon2id with OWASP-recommended parameters.
///
/// Returns the PHC-format hash string (includes algorithm, params, salt, and hash).
///
/// # Errors
///
/// Returns an error if hashing fails.
pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = argon2_instance().hash_password(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

/// Verify a password against a PHC-format hash string.
///
/// # Errors
///
/// Returns an error if the password does not match or the hash is malformed.
pub fn verify_password(password: &str, hash: &str) -> Result<(), argon2::password_hash::Error> {
    let parsed = PasswordHash::new(hash)?;
    argon2_instance().verify_password(password.as_bytes(), &parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_roundtrip() {
        let hash = hash_password("correct-horse-battery-staple").unwrap();
        assert!(hash.starts_with("$argon2id$"));
        verify_password("correct-horse-battery-staple", &hash).unwrap();
    }

    #[test]
    fn wrong_password_fails() {
        let hash = hash_password("correct").unwrap();
        assert!(verify_password("wrong", &hash).is_err());
    }

    #[test]
    fn different_passwords_produce_different_hashes() {
        let h1 = hash_password("password1").unwrap();
        let h2 = hash_password("password2").unwrap();
        assert_ne!(h1, h2);
    }
}
