//! Correlation-ID middleware (issue #865).
//!
//! Every request is tagged with a correlation ID so that all of its log
//! statements can be tied together, across the worker and the services it
//! calls, when an incident is being traced. The generated ID:
//!
//! * is unique per request (nanosecond timestamp + atomic counter),
//! * is carried on a `tracing::Span` for the whole request, so every
//!   `info!` / `warn!` / `error!` emitted by a handler inherits it,
//! * is echoed back in the `x-correlation-id` response header so the caller
//!   can hand it to support when something goes wrong,
//! * is logged next to the final status and total duration of the request.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::{
    extract::Request,
    http::header,
    middleware::Next,
    response::Response,
};
use tracing::{info, info_span, warn, Instrument};

/// Header both sides use to carry the correlation ID. An inbound value is
/// honoured (so a caller can thread its own ID through the whole request
/// chain); otherwise a fresh one is generated.
pub const CORRELATION_ID_HEADER: &str = "x-correlation-id";

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate an opaque, process-unique correlation ID without external
/// dependencies: 16 hex digits of sub-second timestamp + 8 hex digits of a
/// per-process counter. Two calls in the same nanosecond still differ.
pub fn generate_correlation_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:016x}{:08x}", (nanos & 0xFFFF_FFFF_FFFF_FFFF) as u64, counter)
}

/// Extract a caller-supplied correlation ID from a request header, if any.
fn incoming_correlation_id(req: &Request) -> Option<String> {
    req.headers()
        .get(CORRELATION_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// Axum middleware: tag the request with a correlation ID, run the handler
/// inside that span, then log duration/status and echo the ID back.
pub async fn correlate(req: Request, next: Next) -> Response {
    let correlation_id = incoming_correlation_id(&req).unwrap_or_else(generate_correlation_id);
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let started = Instant::now();

    let span = info_span!(
        "http_request",
        correlation_id = %correlation_id,
        method = %method,
        path = %path,
        status = tracing::field::Empty,
        duration_ms = tracing::field::Empty,
    );

    let response = next.run(req).instrument(span.clone()).await;

    let status = response.status().as_u16();
    let duration_ms = started.elapsed().as_millis() as u64;
    span.record("status", &status);
    span.record("duration_ms", &duration_ms);

    // Echo the ID back so callers can reference it when reporting problems.
    let mut response = response;
    if let Ok(value) = header::HeaderValue::from_str(&correlation_id) {
        response
            .headers_mut()
            .insert(CORRELATION_ID_HEADER, value);
    } else {
        warn!(
            correlation_id = %correlation_id,
            "generated correlation ID is not a valid header value"
        );
    }

    info!(
        correlation_id = %correlation_id,
        status,
        duration_ms,
        "request complete"
    );

    response
}