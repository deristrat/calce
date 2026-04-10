use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Arc;

use governor::clock::{Clock, DefaultClock};
use governor::state::keyed::DashMapStateStore;
use governor::{Quota, RateLimiter};

use crate::error::ApiError;

/// Per-IP token bucket rate limiter.
pub(crate) type KeyedRateLimiter = RateLimiter<IpAddr, DashMapStateStore<IpAddr>, DefaultClock>;

/// Create a rate limiter: 10 requests/minute per IP.
#[must_use]
pub(crate) fn create_auth_rate_limiter() -> Arc<KeyedRateLimiter> {
    let quota = Quota::per_minute(NonZeroU32::new(10).unwrap());
    Arc::new(RateLimiter::dashmap(quota))
}

/// Check rate limit for an IP. Returns `Err(ApiError)` with retry-after if limited.
pub(crate) fn check_rate_limit(limiter: &KeyedRateLimiter, ip: IpAddr) -> Result<(), ApiError> {
    limiter.check_key(&ip).map_err(|not_until| {
        let wait = not_until.wait_time_from(DefaultClock::default().now());
        ApiError::RateLimited {
            retry_after_secs: wait.as_secs().saturating_add(1),
        }
    })
}

/// Number of trusted reverse-proxy hops between the client and this service.
/// GCP Cloud Run behind HTTPS LB: set to 1.
const TRUSTED_PROXY_HOPS: usize = 1;

/// Extract client IP from X-Forwarded-For header.
/// Skips `TRUSTED_PROXY_HOPS` entries from the right (added by GCP infrastructure).
pub(crate) fn extract_ip(forwarded_for: Option<&str>) -> IpAddr {
    if let Some(xff) = forwarded_for {
        let parts: Vec<&str> = xff.rsplit(',').map(str::trim).collect();
        // Skip trusted proxy entries from the right; fall back to rightmost
        // if fewer entries than expected (e.g. local dev with single-entry XFF).
        let idx = TRUSTED_PROXY_HOPS.min(parts.len().saturating_sub(1));
        if let Ok(ip) = parts[idx].parse::<IpAddr>() {
            return ip;
        }
    }
    IpAddr::from([127, 0, 0, 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_entry_xff() {
        let ip = extract_ip(Some("203.0.113.50"));
        assert_eq!(ip, "203.0.113.50".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn gcp_lb_xff_skips_lb_ip() {
        // GCP LB appends its own IP as rightmost entry
        let ip = extract_ip(Some("203.0.113.50, 35.191.0.1"));
        assert_eq!(ip, "203.0.113.50".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn spoofed_xff_with_gcp_lb() {
        // Attacker prepends fake IP; GCP adds real client + LB
        let ip = extract_ip(Some("1.2.3.4, 203.0.113.50, 35.191.0.1"));
        assert_eq!(ip, "203.0.113.50".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn no_xff_returns_localhost() {
        assert_eq!(extract_ip(None), IpAddr::from([127, 0, 0, 1]));
    }
}
