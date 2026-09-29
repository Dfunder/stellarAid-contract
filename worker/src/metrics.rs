//! Prometheus-compatible metrics for the worker HTTP service.
//!
//! Implements the `/metrics` scrape endpoint requested in issue #861 without
//! pulling in an external metrics crate: the service needs only a handful of
//! counters and a single latency histogram, so the Prometheus text exposition
//! format is rendered directly from atomics.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use axum::extract::{MatchedPath, Request, State};
use axum::middleware::Next;
use axum::response::Response;

use crate::{AppState, MetricsData};

/// Upper bounds (in seconds) for the request-latency histogram.
pub const LATENCY_BUCKETS_SECONDS: [f64; 8] = [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0];

/// A cumulative Prometheus histogram backed by atomics.
pub struct Histogram {
    buckets: [AtomicU64; LATENCY_BUCKETS_SECONDS.len()],
    count: AtomicU64,
    sum_micros: AtomicU64,
}

impl Default for Histogram {
    fn default() -> Self {
        Self {
            buckets: std::array::from_fn(|_| AtomicU64::new(0)),
            count: AtomicU64::new(0),
            sum_micros: AtomicU64::new(0),
        }
    }
}

impl Histogram {
    /// Record a single observation.
    pub fn observe(&self, elapsed: Duration) {
        let seconds = elapsed.as_secs_f64();
        for (index, upper) in LATENCY_BUCKETS_SECONDS.iter().enumerate() {
            if seconds <= *upper {
                self.buckets[index].fetch_add(1, Ordering::Relaxed);
            }
        }
        self.count.fetch_add(1, Ordering::Relaxed);
        self.sum_micros
            .fetch_add(elapsed.as_micros() as u64, Ordering::Relaxed);
    }

    fn sum_seconds(&self) -> f64 {
        self.sum_micros.load(Ordering::Relaxed) as f64 / 1_000_000.0
    }

    /// Append the `_bucket`, `_sum` and `_count` series for `name`.
    fn render(&self, out: &mut String, name: &str) {
        for (index, upper) in LATENCY_BUCKETS_SECONDS.iter().enumerate() {
            let value = self.buckets[index].load(Ordering::Relaxed);
            out.push_str(&format!("{name}_bucket{{le=\"{upper}\"}} {value}\n"));
        }
        let count = self.count.load(Ordering::Relaxed);
        out.push_str(&format!("{name}_bucket{{le=\"+Inf\"}} {count}\n"));
        out.push_str(&format!("{name}_sum {}\n", self.sum_seconds()));
        out.push_str(&format!("{name}_count {count}\n"));
    }
}

/// Per-route request counters.
#[derive(Default)]
pub struct RouteMetrics {
    pub requests_total: AtomicU64,
    pub errors_total: AtomicU64,
}

/// Process-wide HTTP metrics exposed at `/metrics`.
#[derive(Default)]
pub struct HttpMetrics {
    requests_total: AtomicU64,
    requests_in_flight: AtomicU64,
    errors_total: AtomicU64,
    request_duration_seconds: Histogram,
    routes: RwLock<HashMap<String, RouteMetrics>>,
}

impl HttpMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark a request as started (increments the in-flight gauge).
    pub fn request_started(&self) {
        self.requests_in_flight.fetch_add(1, Ordering::Relaxed);
    }

    /// Record the outcome of a request and update the per-route counters.
    pub fn request_finished(&self, route: &str, status: u16, elapsed: Duration) {
        self.requests_in_flight.fetch_sub(1, Ordering::Relaxed);
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        self.request_duration_seconds.observe(elapsed);

        let is_error = status >= 400;
        if is_error {
            self.errors_total.fetch_add(1, Ordering::Relaxed);
        }

        let mut routes = self.routes.write().unwrap_or_else(|e| e.into_inner());
        let entry = routes.entry(route.to_string()).or_default();
        entry.requests_total.fetch_add(1, Ordering::Relaxed);
        if is_error {
            entry.errors_total.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Render the full Prometheus text exposition for the process.
    pub fn render_prometheus(&self, app: &MetricsData, uptime_seconds: u64) -> String {
        let mut out = String::with_capacity(2048);

        push_metric(
            &mut out,
            "worker_uptime_seconds",
            "gauge",
            "Seconds since the worker process started.",
            uptime_seconds,
        );
        push_metric(
            &mut out,
            "worker_donations_submitted_total",
            "counter",
            "Donation transactions successfully built and returned.",
            app.donations_submitted,
        );
        push_metric(
            &mut out,
            "worker_donations_verified_total",
            "counter",
            "Donation status lookups served.",
            app.donations_verified,
        );
        push_metric(
            &mut out,
            "worker_donation_errors_total",
            "counter",
            "Errors recorded while handling donation operations.",
            app.errors,
        );
        push_metric(
            &mut out,
            "worker_error_log_entries",
            "gauge",
            "Errors currently retained in the in-memory error log.",
            app.error_log.len() as u64,
        );
        push_metric(
            &mut out,
            "worker_http_requests_total",
            "counter",
            "Total HTTP requests handled.",
            self.requests_total.load(Ordering::Relaxed),
        );
        push_metric(
            &mut out,
            "worker_http_requests_in_flight",
            "gauge",
            "HTTP requests currently being processed.",
            self.requests_in_flight.load(Ordering::Relaxed),
        );
        push_metric(
            &mut out,
            "worker_http_errors_total",
            "counter",
            "HTTP responses with a 4xx or 5xx status.",
            self.errors_total.load(Ordering::Relaxed),
        );

        out.push_str(
            "# HELP worker_http_request_duration_seconds HTTP request latency in seconds.\n",
        );
        out.push_str("# TYPE worker_http_request_duration_seconds histogram\n");
        self.request_duration_seconds
            .render(&mut out, "worker_http_request_duration_seconds");

        out.push_str("# HELP worker_route_requests_total HTTP requests per matched route.\n");
        out.push_str("# TYPE worker_route_requests_total counter\n");
        out.push_str("# HELP worker_route_errors_total HTTP errors per matched route.\n");
        out.push_str("# TYPE worker_route_errors_total counter\n");

        let routes = self.routes.read().unwrap_or_else(|e| e.into_inner());
        let mut entries: Vec<(&String, &RouteMetrics)> = routes.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        for (route, metrics) in entries {
            let label = escape_label(route);
            out.push_str(&format!(
                "worker_route_requests_total{{route=\"{label}\"}} {}\n",
                metrics.requests_total.load(Ordering::Relaxed)
            ));
            out.push_str(&format!(
                "worker_route_errors_total{{route=\"{label}\"}} {}\n",
                metrics.errors_total.load(Ordering::Relaxed)
            ));
        }

        out
    }
}

fn push_metric(out: &mut String, name: &str, kind: &str, help: &str, value: u64) {
    out.push_str(&format!("# HELP {name} {help}\n"));
    out.push_str(&format!("# TYPE {name} {kind}\n"));
    out.push_str(&format!("{name} {value}\n"));
}

fn escape_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// Route template for the current request, falling back to the raw path.
fn matched_route(req: &Request) -> String {
    req.extensions()
        .get::<MatchedPath>()
        .map(|matched| matched.as_str().to_string())
        .unwrap_or_else(|| req.uri().path().to_string())
}

/// Middleware that records request counts, error counts and latency.
///
/// Installed with `Router::layer`, so it runs after routing and can read the
/// low-cardinality [`MatchedPath`] template rather than the raw URI.
pub async fn track_metrics(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Response {
    let route = matched_route(&req);
    let started = Instant::now();
    state.http_metrics.request_started();

    let response = next.run(req).await;

    state
        .http_metrics
        .request_finished(&route, response.status().as_u16(), started.elapsed());
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_buckets_are_cumulative() {
        let histogram = Histogram::default();
        histogram.observe(Duration::from_millis(3));
        histogram.observe(Duration::from_millis(20));

        let mut out = String::new();
        histogram.render(&mut out, "h");

        assert!(out.contains("h_bucket{le=\"0.005\"} 1"));
        assert!(out.contains("h_bucket{le=\"0.01\"} 1"));
        assert!(out.contains("h_bucket{le=\"+Inf\"} 2"));
        assert!(out.contains("h_count 2"));
    }

    #[test]
    fn request_finished_tracks_errors_and_routes() {
        let metrics = HttpMetrics::new();
        metrics.request_started();
        metrics.request_finished("/health", 200, Duration::from_millis(1));
        metrics.request_finished("/api/donations/submit", 500, Duration::from_millis(2));

        let rendered = metrics.render_prometheus(&MetricsData::default(), 42);

        assert!(rendered.contains("worker_http_requests_total 2"));
        assert!(rendered.contains("worker_http_errors_total 1"));
        assert!(rendered.contains("worker_route_requests_total{route=\"/health\"} 1"));
        assert!(rendered.contains("worker_route_errors_total{route=\"/api/donations/submit\"} 1"));
        assert!(rendered.contains("worker_uptime_seconds 42"));
    }

    #[test]
    fn in_flight_gauge_returns_to_zero() {
        let metrics = HttpMetrics::new();
        metrics.request_started();
        metrics.request_finished("/health", 200, Duration::from_millis(1));

        assert!(metrics
            .render_prometheus(&MetricsData::default(), 0)
            .contains("worker_http_requests_in_flight 0"));
    }
}
