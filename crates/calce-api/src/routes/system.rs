//! System monitoring endpoints — admin-only overview of services and config.

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::routing::get;
use calce_data::auth::authz::require_admin;
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::auth::Auth;
use crate::error::ApiError;
use crate::state::AppState;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/admin/system/info", get(info))
        .route("/v1/admin/system/config", get(config))
}

// ── /info ──────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct SystemInfo {
    api: ApiInfo,
    services: Vec<ServiceInfo>,
    components: Vec<ComponentInfo>,
}

#[derive(Serialize)]
struct ApiInfo {
    version: &'static str,
    started_at: DateTime<Utc>,
    target: String,
    profile: &'static str,
}

/// A separately-addressable service in the Calce deployment.
#[derive(Serialize)]
struct ServiceInfo {
    name: &'static str,
    role: &'static str,
    url: String,
    /// `None` means: status is determined by the client (e.g. probing the URL).
    status: Option<&'static str>,
}

/// An in-process component of calce-api (pubsub, simulator, CDC, etc.).
#[derive(Serialize)]
struct ComponentInfo {
    name: &'static str,
    status: &'static str,
    detail: Option<String>,
}

async fn info(
    Auth(ctx): Auth,
    State(state): State<AppState>,
) -> Result<Json<SystemInfo>, ApiError> {
    require_admin(&ctx)?;
    Ok(Json(SystemInfo {
        api: api_info(&state),
        services: service_list(&state),
        components: component_list(&state),
    }))
}

fn api_info(state: &AppState) -> ApiInfo {
    ApiInfo {
        version: env!("CARGO_PKG_VERSION"),
        started_at: state.started_at,
        target: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
    }
}

fn service_list(state: &AppState) -> Vec<ServiceInfo> {
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://calce:calce@localhost:5433/calce".into());
    let ai_url = std::env::var("CALCE_AI_URL").unwrap_or_else(|_| "http://localhost:35801".into());
    vec![
        ServiceInfo {
            name: "calce-api",
            role: "HTTP API + calculation engine",
            url: self_url(),
            status: Some("ok"),
        },
        ServiceInfo {
            name: "calce-ai",
            role: "AI financial analyst (Python)",
            url: ai_url,
            status: None,
        },
        ServiceInfo {
            name: "calce-console",
            role: "Admin console (this UI)",
            url: "(browser origin)".into(),
            status: None,
        },
        ServiceInfo {
            name: "postgres",
            role: "Primary database",
            url: mask_database_url(&db_url),
            status: Some(if state.pool.is_some() {
                "connected"
            } else {
                "disabled"
            }),
        },
    ]
}

fn component_list(state: &AppState) -> Vec<ComponentInfo> {
    let running_if = |present: bool| if present { "running" } else { "disabled" };
    let available_if = |present: bool| if present { "available" } else { "disabled" };
    vec![
        ComponentInfo {
            name: "CDC (logical replication)",
            status: if cdc_enabled() { "enabled" } else { "disabled" },
            detail: None,
        },
        ComponentInfo {
            name: "PubSub: price",
            status: running_if(state.price_pubsub.is_some()),
            detail: None,
        },
        ComponentInfo {
            name: "PubSub: fx",
            status: running_if(state.fx_pubsub.is_some()),
            detail: None,
        },
        ComponentInfo {
            name: "PubSub: entity",
            status: running_if(state.entity_pubsub.is_some()),
            detail: None,
        },
        ComponentInfo {
            name: "Price simulator",
            status: available_if(state.simulator.is_some()),
            detail: None,
        },
        ComponentInfo {
            name: "DB simulator",
            status: available_if(state.db_simulator.is_some()),
            detail: None,
        },
    ]
}

fn self_url() -> String {
    let port = std::env::var("PORT").unwrap_or_else(|_| "35701".into());
    format!("http://localhost:{port}")
}

fn cdc_enabled() -> bool {
    std::env::var("CALCE_CDC_ENABLED")
        .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
        .unwrap_or(true)
}

// ── /config ────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ConfigResponse {
    entries: Vec<ConfigEntry>,
}

#[derive(Serialize)]
struct ConfigEntry {
    key: &'static str,
    group: &'static str,
    description: &'static str,
    /// Rendered value (plain for non-secrets, masked for secrets, `None` if unset).
    value: Option<String>,
    secret: bool,
}

/// Known env vars we want to surface. Listed explicitly so we never leak
/// an unknown variable that happens to be in the process environment.
const KNOWN_CONFIG: &[(&str, &str, &str, ConfigKind)] = &[
    (
        "DATABASE_URL",
        "Database",
        "Postgres connection URL",
        ConfigKind::DatabaseUrl,
    ),
    (
        "CALCE_JWT_PRIVATE_KEY",
        "Auth",
        "Ed25519 PKCS#8 DER (base64) — signs access tokens",
        ConfigKind::Secret,
    ),
    (
        "CALCE_HMAC_SECRET",
        "Auth",
        "HMAC key — hashes refresh tokens + API keys",
        ConfigKind::Secret,
    ),
    (
        "ANTHROPIC_API_KEY",
        "Integrations",
        "Claude API key used by calce-ai",
        ConfigKind::Secret,
    ),
    (
        "CALCE_AI_URL",
        "Integrations",
        "Base URL for the calce-ai service",
        ConfigKind::Plain,
    ),
    (
        "CALCE_CDC_ENABLED",
        "Runtime",
        "Enable logical-replication CDC listener",
        ConfigKind::Plain,
    ),
    ("PORT", "Runtime", "HTTP server port", ConfigKind::Plain),
    (
        "RUST_LOG",
        "Runtime",
        "Tracing filter directive",
        ConfigKind::Plain,
    ),
];

#[derive(Clone, Copy)]
enum ConfigKind {
    Plain,
    Secret,
    DatabaseUrl,
}

async fn config(
    Auth(ctx): Auth,
    State(_state): State<AppState>,
) -> Result<Json<ConfigResponse>, ApiError> {
    require_admin(&ctx)?;

    let entries = KNOWN_CONFIG
        .iter()
        .map(|&(key, group, description, kind)| {
            let raw = std::env::var(key).ok();
            let value = raw.as_deref().map(|v| match kind {
                ConfigKind::Plain => v.to_owned(),
                ConfigKind::Secret => mask_secret(v),
                ConfigKind::DatabaseUrl => mask_database_url(v),
            });
            ConfigEntry {
                key,
                group,
                description,
                value,
                secret: matches!(kind, ConfigKind::Secret | ConfigKind::DatabaseUrl),
            }
        })
        .collect();

    Ok(Json(ConfigResponse { entries }))
}

// ── Masking helpers ────────────────────────────────────────────────────

/// Show the first 8 ASCII chars of a secret plus its total length.
/// Values of 8 chars or fewer are fully masked.
fn mask_secret(s: &str) -> String {
    let len = s.len();
    if len <= 8 {
        return "*".repeat(len);
    }
    // Walk char boundaries in case the value is not pure ASCII.
    let prefix: String = s.chars().take(8).collect();
    format!("{prefix}…  ({len} chars)")
}

/// Replace the password component in a URL like `scheme://user:pass@host/db`.
fn mask_database_url(url: &str) -> String {
    let Some((before_at, after_at)) = url.split_once('@') else {
        return url.to_owned();
    };
    let Some((scheme_user, _pass)) = before_at.rsplit_once(':') else {
        return url.to_owned();
    };
    format!("{scheme_user}:****@{after_at}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_secret_short_value_is_fully_hidden() {
        assert_eq!(mask_secret(""), "");
        assert_eq!(mask_secret("short"), "*****");
        assert_eq!(mask_secret("eightchr"), "********");
    }

    #[test]
    fn mask_secret_long_value_shows_prefix() {
        let out = mask_secret("abcdefghijklmnop");
        assert!(out.starts_with("abcdefgh"));
        assert!(out.contains("16"));
    }

    #[test]
    fn mask_database_url_hides_password() {
        let url = "postgres://calce:supersecret@localhost:5433/calce";
        assert_eq!(
            mask_database_url(url),
            "postgres://calce:****@localhost:5433/calce"
        );
    }

    #[test]
    fn mask_database_url_without_auth_is_unchanged() {
        let url = "postgres://localhost:5433/calce";
        assert_eq!(mask_database_url(url), url);
    }
}
