use chrono::{Duration, Utc};
use pyo3::prelude::*;
use uuid::Uuid;

use calce_data::auth::jwt::ACCESS_TOKEN_LIFETIME_SECS;
use calce_data::auth::{
    AuthConfig, DUMMY_PASSWORD_HASH, GRACE_PERIOD_SECS, LOCKOUT_DURATION_MINUTES,
    LOCKOUT_THRESHOLD, MAX_PASSWORD_LENGTH, REFRESH_TOKEN_LIFETIME_DAYS, Role, jwt, password,
    tokens,
};
use calce_data::queries::auth::AuthRepo;

use crate::errors::DataLoadError;

pyo3::create_exception!(calce, AuthError, pyo3::exceptions::PyException);
pyo3::create_exception!(calce, InvalidCredentialsError, AuthError);
pyo3::create_exception!(calce, AccountLockedError, AuthError);
pyo3::create_exception!(calce, InvalidTokenError, AuthError);

#[pyclass(frozen, name = "SecurityContext")]
pub struct PySecurityContext {
    #[pyo3(get)]
    pub user_id: String,
    #[pyo3(get)]
    pub role: String,
    #[pyo3(get)]
    pub org_id: Option<String>,
}

#[pymethods]
impl PySecurityContext {
    fn __repr__(&self) -> String {
        format!(
            "SecurityContext(user_id={:?}, role={:?})",
            self.user_id, self.role
        )
    }
}

#[pyclass(frozen, name = "TokenPair")]
pub struct PyTokenPair {
    #[pyo3(get)]
    pub access_token: String,
    #[pyo3(get)]
    pub refresh_token: String,
    #[pyo3(get)]
    pub expires_in: u64,
}

#[pyclass]
pub struct AuthService {
    rt: tokio::runtime::Runtime,
    pool: sqlx::PgPool,
    config: AuthConfig,
}

#[pymethods]
impl AuthService {
    #[new]
    fn new(database_url: &str) -> PyResult<Self> {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| DataLoadError::new_err(format!("Failed to create async runtime: {e}")))?;

        let pool = rt
            .block_on(calce_data::config::create_pool(Some(database_url)))
            .map_err(|e| DataLoadError::new_err(format!("Failed to connect to database: {e}")))?;

        let config = AuthConfig::from_env();

        Ok(Self { rt, pool, config })
    }

    /// Authenticate with email and password. Returns a `TokenPair`.
    ///
    /// # Errors
    ///
    /// - `InvalidCredentialsError` if the email or password is wrong.
    /// - `AccountLockedError` if too many failed attempts.
    /// - `AuthError` for other failures.
    fn login(&self, email: &str, password: &str) -> PyResult<PyTokenPair> {
        if password.len() > MAX_PASSWORD_LENGTH {
            return Err(InvalidCredentialsError::new_err(
                "Invalid email or password",
            ));
        }

        self.rt.block_on(async {
            let cred = AuthRepo::find_credential_by_email(&self.pool, email)
                .await
                .map_err(|e| AuthError::new_err(format!("Database error: {e}")))?;

            let (cred, password_ok) = if let Some(c) = cred {
                let ok = password::verify_password(password, &c.password_hash).is_ok();
                (Some(c), ok)
            } else {
                let _ = password::verify_password(password, DUMMY_PASSWORD_HASH);
                (None, false)
            };

            let cred = match (cred, password_ok) {
                (Some(c), true) => c,
                (Some(c), false) => {
                    self.record_failed_attempt(c.credential_id, c.failed_attempts)
                        .await?;
                    return Err(InvalidCredentialsError::new_err(
                        "Invalid email or password",
                    ));
                }
                _ => {
                    return Err(InvalidCredentialsError::new_err(
                        "Invalid email or password",
                    ));
                }
            };

            // Check lockout
            if let Some(locked_until) = cred.locked_until
                && locked_until > Utc::now()
            {
                return Err(AccountLockedError::new_err(format!(
                    "Account locked until {locked_until}"
                )));
            }

            if cred.failed_attempts > 0 {
                AuthRepo::reset_failed_attempts(&self.pool, cred.credential_id)
                    .await
                    .map_err(|e| AuthError::new_err(format!("Database error: {e}")))?;
            }

            let role = Role::parse(&cred.role);
            let family_id = Uuid::new_v4();
            self.issue_tokens(
                cred.user_internal_id,
                &cred.user_external_id,
                &role,
                family_id,
            )
            .await
        })
    }

    /// Validate a JWT access token. Returns a `SecurityContext`.
    ///
    /// # Errors
    ///
    /// - `InvalidTokenError` if the token is invalid or expired.
    fn validate_token(&self, token: &str) -> PyResult<PySecurityContext> {
        let ctx = jwt::decode_access_token(token, &self.config.jwt_decoding_key)
            .map_err(|_| InvalidTokenError::new_err("Invalid or expired token"))?;

        Ok(PySecurityContext {
            user_id: ctx.user_id.as_str().to_owned(),
            role: if ctx.is_admin() {
                "admin".to_owned()
            } else {
                "user".to_owned()
            },
            org_id: ctx.org_id,
        })
    }

    /// Exchange a refresh token for a new token pair.
    ///
    /// # Errors
    ///
    /// - `InvalidTokenError` if the refresh token is invalid, expired, or revoked.
    fn refresh(&self, refresh_token: &str) -> PyResult<PyTokenPair> {
        self.rt.block_on(async {
            let token_hash = tokens::hmac_hash(refresh_token, &self.config.hmac_secret);

            let row = AuthRepo::find_refresh_token(&self.pool, &token_hash)
                .await
                .map_err(|e| AuthError::new_err(format!("Database error: {e}")))?
                .ok_or_else(|| InvalidTokenError::new_err("Invalid refresh token"))?;

            if row.revoked_at.is_some() || row.expires_at < Utc::now() {
                return Err(InvalidTokenError::new_err("Invalid refresh token"));
            }

            // Already superseded — check grace period
            if let Some(superseded_at) = row.superseded_at {
                let elapsed = Utc::now() - superseded_at;
                if elapsed.num_seconds() < GRACE_PERIOD_SECS {
                    let role = Role::parse(&row.user_role);
                    let access_token = jwt::encode_access_token(
                        &row.user_external_id,
                        &role,
                        None,
                        &self.config.jwt_encoding_key,
                    )
                    .map_err(|e| AuthError::new_err(format!("Token error: {e}")))?;

                    return Ok(PyTokenPair {
                        access_token,
                        refresh_token: refresh_token.to_owned(),
                        expires_in: ACCESS_TOKEN_LIFETIME_SECS,
                    });
                }
                // Replay attack — revoke family
                AuthRepo::revoke_token_family(&self.pool, row.family_id)
                    .await
                    .map_err(|e| AuthError::new_err(format!("Database error: {e}")))?;
                return Err(InvalidTokenError::new_err(
                    "Refresh token reuse detected — session revoked",
                ));
            }

            // Active token — rotate
            AuthRepo::supersede_refresh_token(&self.pool, row.id)
                .await
                .map_err(|e| AuthError::new_err(format!("Database error: {e}")))?;

            let role = Role::parse(&row.user_role);
            self.issue_tokens(row.user_id, &row.user_external_id, &role, row.family_id)
                .await
        })
    }

    /// Revoke all tokens in the family associated with this refresh token.
    fn logout(&self, refresh_token: &str) -> PyResult<()> {
        self.rt.block_on(async {
            let token_hash = tokens::hmac_hash(refresh_token, &self.config.hmac_secret);
            AuthRepo::revoke_family_by_token_hash(&self.pool, &token_hash)
                .await
                .map_err(|e| AuthError::new_err(format!("Database error: {e}")))?;
            Ok(())
        })
    }
}

impl AuthService {
    async fn issue_tokens(
        &self,
        user_internal_id: i64,
        user_external_id: &str,
        role: &Role,
        family_id: Uuid,
    ) -> PyResult<PyTokenPair> {
        let access_token =
            jwt::encode_access_token(user_external_id, role, None, &self.config.jwt_encoding_key)
                .map_err(|e| AuthError::new_err(format!("Token error: {e}")))?;

        let refresh_value = tokens::generate_token();
        let token_hash = tokens::hmac_hash(&refresh_value, &self.config.hmac_secret);
        let expires_at = Utc::now() + Duration::days(REFRESH_TOKEN_LIFETIME_DAYS);

        AuthRepo::create_refresh_token(
            &self.pool,
            user_internal_id,
            family_id,
            &token_hash,
            expires_at,
        )
        .await
        .map_err(|e| AuthError::new_err(format!("Database error: {e}")))?;

        Ok(PyTokenPair {
            access_token,
            refresh_token: refresh_value,
            expires_in: ACCESS_TOKEN_LIFETIME_SECS,
        })
    }

    async fn record_failed_attempt(
        &self,
        credential_id: i64,
        current_attempts: i32,
    ) -> PyResult<()> {
        let new_count = current_attempts + 1;
        if new_count >= LOCKOUT_THRESHOLD {
            let until = Utc::now() + Duration::minutes(LOCKOUT_DURATION_MINUTES);
            AuthRepo::lock_account(&self.pool, credential_id, until)
                .await
                .map_err(|e| AuthError::new_err(format!("Database error: {e}")))?;
        } else {
            AuthRepo::increment_failed_attempts(&self.pool, credential_id)
                .await
                .map_err(|e| AuthError::new_err(format!("Database error: {e}")))?;
        }
        Ok(())
    }
}

pub(crate) fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_class::<AuthService>()?;
    parent.add_class::<PySecurityContext>()?;
    parent.add_class::<PyTokenPair>()?;
    parent.add("AuthError", parent.py().get_type::<AuthError>())?;
    parent.add(
        "InvalidCredentialsError",
        parent.py().get_type::<InvalidCredentialsError>(),
    )?;
    parent.add(
        "AccountLockedError",
        parent.py().get_type::<AccountLockedError>(),
    )?;
    parent.add(
        "InvalidTokenError",
        parent.py().get_type::<InvalidTokenError>(),
    )?;
    Ok(())
}
