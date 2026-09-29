//! Per-client, per-endpoint rate limiting for the worker HTTP service.
//!
//! Closes issue #864. Implemented in-crate rather than with `tower::limit` so
//! that we can return a proper `429 Too Many Requests` with a `Retry-After`
//! header and vary the budget per endpoint. Client identity is taken from
//! `X-Forwarded-For` / `X-Real-IP` when present (the worker normally sits
//! behind a proxy), falling back to the TCP peer address.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, MatchedPath, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use tracing::warn;

use crate::AppState;

/// A fixed-window rate limit: at most `max_requests` per client per `window`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimit {
    pub max_requests: u32,
    pub window: Duration,
}

impl RateLimit {
    pub const fn new(max_requests: u32, window: Duration) -> Self {
        Self {
            max_requests,
            window,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Window {
    count: u32,
    reset_at: Instant,
}

/// Tracks request windows keyed by `client|route`.
pub struct RateLimiter {
    default: RateLimit,
    per_route: HashMap<String, RateLimit>,
    windows: Mutex<HashMap<String, Window>>,
}

impl RateLimiter {
    pub fn new(default: RateLimit, per_route: HashMap<String, RateLimit>) -> Self {
        Self {
            default,
            per_route,
            windows: Mutex::new(HashMap::new()),
        }
    }

    fn limit_for(&self, route: &str) -> RateLimit {
        self.per_route.get(route).copied().unwrap_or(self.default)
    }

    /// Records a request and reports whether it is allowed.
    ///
    /// `Err(retry_after_seconds)` means the caller must respond `429` and set
    /// `Retry-After` to the returned number of seconds.
    pub fn check(&self, client: &str, route: &str) -> Result<(), u64> {
        let limit = self.limit_for(route);
        let now = Instant::now();
        let key = format!("{client}|{route}");

        let mut windows = self.windows.lock().unwrap_or_else(|e| e.into_inner());

        // Bound memory: drop elapsed windows whenever the table grows beyond a
        // small threshold. A background task also prunes periodically.
        if windows.len() > 4096 {
            windows.retain(|_, window| window.reset_at > now);
        }

        let entry = windows.entry(key).or_insert(Window {
            count: 0,
            reset_at: now + limit.window,
        });

        if now >= entry.reset_at {
            entry.count = 0;
            entry.reset_at = now + limit.window;
        }

        if entry.count >= limit.max_requests {
            let remaining = entry.reset_at.saturating_duration_since(now);
            let retry_after = (remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0)).max(1);
            return Err(retry_after);
        }

        entry.count += 1;
        Ok(())
    }

    /// Drops windows that have already elapsed.
    pub fn prune(&self) {
        let now = Instant::now();
        let mut windows = self.windows.lock().unwrap_or_else(|e| e.into_inner());
        windows.retain(|_, window| window.reset_at > now);
    }
}

/// Axum middleware enforcing the configured rate limits.
pub async fn rate_limit(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Response {
    let route = matched_route(&req);
    let client = client_identity(&req);

    match state.rate_limiter.check(&client, &route) {
        Ok(()) => next.run(req).await,
        Err(retry_after) => {
            warn!(
                client = %client,
                route = %route,
                retry_after,
                "rate limit exceeded"
            );
            let body = json!({
                "error": "rate limit exceeded",
                "retry_after": retry_after,
            });
            (
                StatusCode::TOO_MANY_REQUESTS,
                [(header::RETRY_AFTER, retry_after.to_string())],
                Json(body),
            )
                .into_response()
        }
    }
}

fn matched_route(req: &Request) -> String {
    req.extensions()
        .get::<MatchedPath>()
        .map(|matched| matched.as_str().to_string())
        .unwrap_or_else(|| req.uri().path().to_string())
}

fn client_identity(req: &Request) -> String {
    if let Some(ip) = forwarded_ip(req.headers()) {
        return ip;
    }
    if let Some(ConnectInfo(addr)) = req.extensions().get::<ConnectInfo<SocketAddr>>() {
        return addr.ip().to_string();
    }
    "unknown".to_string()
}

fn forwarded_ip(headers: &HeaderMap) -> Option<String> {
    for name in ["x-forwarded-for", "x-real-ip"] {
        if let Some(value) = headers.get(name).and_then(|value| value.to_str().ok()) {
            // `X-Forwarded-For` is a comma-separated list; the first entry is the
            // original client.
            let candidate = value.split(',').next().unwrap_or_default().trim();
            if let Ok(ip) = candidate.parse::<IpAddr>() {
                return Some(ip.to_string());
            }
            if let Ok(addr) = candidate.parse::<SocketAddr>() {
                return Some(addr.ip().to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn limiter() -> RateLimiter {
        RateLimiter::new(RateLimit::new(2, Duration::from_secs(60)), HashMap::new())
    }

    #[test]
    fn allows_up_to_limit_then_rejects() {
        let limiter = limiter();
        assert!(limiter.check("1.2.3.4", "/health").is_ok());
        assert!(limiter.check("1.2.3.4", "/health").is_ok());
        let retry_after = limiter.check("1.2.3.4", "/health").unwrap_err();
        assert!(retry_after >= 1);
    }

    #[test]
    fn limits_are_per_client() {
        let limiter = limiter();
        assert!(limiter.check("1.1.1.1", "/health").is_ok());
        assert!(limiter.check("1.1.1.1", "/health").is_ok());
        assert!(limiter.check("1.1.1.1", "/health").is_err());
        // A different client keeps its own window.
        assert!(limiter.check("2.2.2.2", "/health").is_ok());
    }

    #[test]
    fn per_route_override_is_applied() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "/api/donations/submit".to_string(),
            RateLimit::new(1, Duration::from_secs(60)),
        );
        let limiter = RateLimiter::new(RateLimit::new(100, Duration::from_secs(60)), overrides);

        assert!(limiter.check("9.9.9.9", "/api/donations/submit").is_ok());
        assert!(limiter.check("9.9.9.9", "/api/donations/submit").is_err());
        // Other endpoints keep the generous default.
        assert!(limiter.check("9.9.9.9", "/health").is_ok());
    }

    #[test]
    fn window_resets_after_expiry() {
        let limiter = RateLimiter::new(RateLimit::new(1, Duration::from_millis(1)), HashMap::new());
        assert!(limiter.check("3.3.3.3", "/x").is_ok());
        assert!(limiter.check("3.3.3.3", "/x").is_err());
        std::thread::sleep(Duration::from_millis(5));
        assert!(limiter.check("3.3.3.3", "/x").is_ok());
    }

    #[test]
    fn forwarded_ip_prefers_first_hop() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("203.0.113.7, 10.0.0.1"),
        );
        assert_eq!(forwarded_ip(&headers).as_deref(), Some("203.0.113.7"));
    }

    #[test]
    fn forwarded_ip_ignores_garbage() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("not-an-ip"));
        assert_eq!(forwarded_ip(&headers), None);
    }
}
