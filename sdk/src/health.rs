//! Typed endpoint health for the SDK's network clients (#980).
//!
//! [`crate::soroban::rpc_client::SorobanRpcClient::health`] and
//! [`crate::horizon::client::HorizonClient::health`] return an [`EndpointHealth`]
//! so a deployment script can tell *which* of the two endpoints is reachable
//! before using it. This is for setup and diagnostics; it is deliberately **not**
//! a precondition on every SDK operation.
//!
//! Distinguishing outcomes:
//!
//! * `Ok(EndpointHealth { healthy: true, .. })` — the endpoint answered.
//! * `Ok(EndpointHealth { healthy: false, .. })` — the endpoint answered but
//!   reported itself unhealthy (e.g. a non-2xx status).
//! * `Err(StellarAidError::EndpointUnreachable | RequestTimeout)` — the endpoint
//!   did not answer at all. These are the retryable variants.

use std::time::Duration;

/// Timeout applied to a single health probe. Short by design, so a probe can
/// never hang the caller.
pub const HEALTH_PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// Result of probing a single endpoint.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EndpointHealth {
    /// The URL that was probed.
    pub endpoint: String,
    /// Whether the endpoint reported itself healthy.
    pub healthy: bool,
    /// Round-trip time for the probe, in milliseconds.
    pub latency_ms: u128,
    /// Extra context (server message or status) when `healthy` is false.
    pub detail: Option<String>,
}

impl EndpointHealth {
    /// A successful probe.
    pub fn up(endpoint: impl Into<String>, latency_ms: u128) -> Self {
        Self {
            endpoint: endpoint.into(),
            healthy: true,
            latency_ms,
            detail: None,
        }
    }

    /// The endpoint answered, but is not healthy.
    pub fn down(
        endpoint: impl Into<String>,
        detail: impl Into<String>,
        latency_ms: u128,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            healthy: false,
            latency_ms,
            detail: Some(detail.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_has_no_detail() {
        let h = EndpointHealth::up("https://rpc", 12);
        assert!(h.healthy);
        assert_eq!(h.latency_ms, 12);
        assert!(h.detail.is_none());
    }

    #[test]
    fn down_carries_detail() {
        let h = EndpointHealth::down("https://rpc", "HTTP 503", 7);
        assert!(!h.healthy);
        assert_eq!(h.detail.as_deref(), Some("HTTP 503"));
    }
}
