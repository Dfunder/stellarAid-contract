mod correlation;
mod metrics;
mod ratelimit;
mod webhooks;
pub mod db;
pub mod models;
pub mod services;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    middleware,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use sdk::{
    errors::StellarAidError,
    retry::{retry_async, RetryConfig},
    soroban::rpc_client::SorobanRpcClient,
    transaction_builder::{build_donate_transaction_full, DonationParams, NetworkConfig},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::timeout::TimeoutLayer;
use tracing::{info, warn};
use webhooks::WebhookManager;

use crate::correlation::{correlate, CORRELATION_ID_HEADER};
use crate::metrics::{track_metrics, HttpMetrics};
use crate::ratelimit::{rate_limit, RateLimit, RateLimiter};

#[derive(Debug, Deserialize)]
pub struct SubmitDonationRequest {
    pub donor: String,
    pub campaign_id: u64,
    pub amount: i128,
    pub token_address: Option<String>,
    pub anonymous: Option<bool>,
    pub memo: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SubmitDonationResponse {
    pub xdr: String,
    pub donation_contract_id: String,
    pub network_passphrase: String,
}

#[derive(Debug, Serialize)]
pub struct DonationInfo {
    pub tx_hash: String,
    pub donor: String,
    pub campaign_id: u64,
    pub amount: i128,
    pub status: String,
    pub memo: Option<String>,
    pub anonymous: bool,
    pub token_address: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: &'static str,
    pub uptime_seconds: u64,
    pub metrics: MetricsSummary,
}

#[derive(Debug, Serialize)]
pub struct MetricsSummary {
    pub total_donations_submitted: u64,
    pub total_donations_verified: u64,
    pub total_errors: u64,
    pub recent_errors: Vec<String>,
    pub rpc_connected: bool,
}

#[derive(Clone)]
pub struct AppState {
    pub network_config: NetworkConfig,
    pub donation_contract_id: String,
    pub webhook_manager: WebhookManager,
    pub metrics: Arc<RwLock<MetricsData>>,
    pub startup_time: Instant,
    pub http_metrics: Arc<HttpMetrics>,
    pub rate_limiter: Arc<RateLimiter>,
}

#[derive(Clone, Default)]
pub struct MetricsData {
    pub donations_submitted: u64,
    pub donations_verified: u64,
    pub errors: u64,
    pub error_log: Vec<String>,
}

/// Record an error in the shared metrics. Centralised so the lock is always
/// awaited — `tokio::sync::RwLock::blocking_write` panics inside the runtime.
async fn record_error(state: &AppState, message: String) {
    let mut metrics = state.metrics.write().await;
    metrics.errors += 1;
    metrics.error_log.push(message);
}

/// Read a `u64` environment variable, falling back to `default` when it is
/// unset or unparsable.
fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(default)
}

/// Route-specific rate limits.
///
/// The submission endpoint is the most expensive operation, so it gets a
/// tighter budget. Operational endpoints (probes and scrapes) get a much larger
/// one so monitoring is never throttled off by the default limit.
fn build_route_limits() -> HashMap<String, RateLimit> {
    let window = Duration::from_secs(env_u64("RATE_LIMIT_WINDOW_SECS", 60));
    let mut limits = HashMap::new();
    limits.insert(
        "/api/donations/submit".to_string(),
        RateLimit::new(env_u64("SUBMIT_RATE_LIMIT_MAX", 10) as u32, window),
    );
    for route in ["/health", "/ready", "/metrics"] {
        limits.insert(
            route.to_string(),
            RateLimit::new(env_u64("OPS_RATE_LIMIT_MAX", 600) as u32, window),
        );
    }
    limits
}

/// Resolves when the process receives SIGINT or SIGTERM.
///
/// Once a signal arrives a watchdog is armed: if in-flight requests have not
/// drained within `timeout`, the process exits anyway so a stuck request cannot
/// block shutdown indefinitely (issue #862).
async fn shutdown_signal(done: Arc<AtomicBool>, timeout: Duration) {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            warn!(error = %err, "failed to install Ctrl+C handler");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(err) => warn!(error = %err, "failed to install SIGTERM handler"),
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => info!(signal = "SIGINT", "shutdown signal received"),
        _ = terminate => info!(signal = "SIGTERM", "shutdown signal received"),
    }

    tokio::spawn(async move {
        tokio::time::sleep(timeout).await;
        if !done.load(Ordering::SeqCst) {
            warn!(
                timeout_secs = timeout.as_secs(),
                "graceful shutdown timed out, forcing exit"
            );
            std::process::exit(0);
        }
    });
}

async fn submit_donation(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SubmitDonationRequest>,
) -> Result<Json<SubmitDonationResponse>, (StatusCode, Json<ErrorResponse>)> {
    if req.amount <= 0 {
        record_error(&state, "submit_donation: amount must be positive".to_string()).await;
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "amount must be positive".to_string(),
            }),
        ));
    }

    let campaign_id = req.campaign_id;
    info!(
        campaign_id,
        amount = %req.amount,
        "submit_donation: building transaction"
    );

    let params = DonationParams {
        donor: req.donor,
        campaign_id: req.campaign_id,
        amount: req.amount,
        token_address: req.token_address,
        anonymous: req.anonymous.unwrap_or(false),
        memo: req.memo,
        donation_contract_id: state.donation_contract_id.clone(),
    };

    let retry_config = RetryConfig::default();
    let network = state.network_config.clone();

    let xdr = match retry_async(&retry_config, || async {
        build_donate_transaction_full(&params, &network)
            .await
            .map_err(|e| StellarAidError::SorobanError(e.to_string()))
    })
    .await
    {
        Ok(xdr) => xdr,
        Err(e) => {
            record_error(&state, format!("submit_donation: {}", e)).await;
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("transaction build failed: {}", e),
                }),
            ));
        }
    };

    let mut metrics = state.metrics.write().await;
    metrics.donations_submitted += 1;

    info!(campaign_id, "submit_donation: transaction built successfully");

    Ok(Json(SubmitDonationResponse {
        xdr,
        donation_contract_id: state.donation_contract_id.clone(),
        network_passphrase: state.network_config.network_passphrase.clone(),
    }))
}

async fn get_donation(
    State(state): State<Arc<AppState>>,
    Path(tx_hash): Path<String>,
) -> Result<Json<DonationInfo>, (StatusCode, Json<ErrorResponse>)> {
    let rpc = SorobanRpcClient::new(&state.network_config.rpc_url);

    info!(tx_hash = %tx_hash, "get_donation: querying transaction status");

    let status = match retry_async(&RetryConfig::default(), || async {
        rpc.get_transaction_status(&tx_hash)
            .await
            .map_err(|e| StellarAidError::SorobanError(e.to_string()))
    })
    .await
    {
        Ok(status) => status,
        Err(e) => {
            record_error(&state, format!("get_donation: {}", e)).await;
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("failed to get transaction status: {}", e),
                }),
            ));
        }
    };

    let mut metrics = state.metrics.write().await;
    metrics.donations_verified += 1;

    let status_str = match status {
        sdk::soroban::rpc_client::TransactionStatus::Pending => "pending".to_string(),
        sdk::soroban::rpc_client::TransactionStatus::Success => "success".to_string(),
        sdk::soroban::rpc_client::TransactionStatus::Failed => "failed".to_string(),
        sdk::soroban::rpc_client::TransactionStatus::NotFound => "not_found".to_string(),
    };

    Ok(Json(DonationInfo {
        tx_hash: tx_hash.clone(),
        donor: String::new(),
        campaign_id: 0,
        amount: 0,
        status: status_str,
        memo: None,
        anonymous: false,
        token_address: None,
    }))
}

/// Health endpoint that returns service status and operational metrics.
/// Used by monitoring systems and load balancers.
async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let metrics = state.metrics.read().await;
    let recent_errors: Vec<String> = metrics.error_log.iter()
        .rev()
        .take(10)
        .cloned()
        .collect();

    // Quick RPC connectivity check
    let rpc_connected = SorobanRpcClient::new(&state.network_config.rpc_url)
        .get_health()
        .await
        .is_ok();

    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION"),
        uptime_seconds: state.startup_time.elapsed().as_secs(),
        metrics: MetricsSummary {
            total_donations_submitted: metrics.donations_submitted,
            total_donations_verified: metrics.donations_verified,
            total_errors: metrics.errors,
            recent_errors,
            rpc_connected,
        },
    })
}

/// Readiness endpoint for load balancer checks.
async fn readiness(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let rpc_connected = SorobanRpcClient::new(&state.network_config.rpc_url)
        .get_health()
        .await
        .is_ok();
    let ready = rpc_connected;

    Json(serde_json::json!({
        "ready": ready,
        "rpc_connected": rpc_connected,
    }))
}

/// Prometheus scrape endpoint (issue #861).
async fn metrics_endpoint(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let app_metrics = state.metrics.read().await;
    let body = state
        .http_metrics
        .render_prometheus(&app_metrics, state.startup_time.elapsed().as_secs());

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
}

#[tokio::main]
async fn main() {
    let _ = logging::init_logging();
    info!(event = "worker_startup", "StellarAid worker starting");

    let network_config = NetworkConfig {
        rpc_url: std::env::var("SOROBAN_RPC_URL")
            .unwrap_or_else(|_| "https://soroban-testnet.stellar.org".to_string()),
        horizon_url: std::env::var("HORIZON_URL")
            .unwrap_or_else(|_| "https://horizon-testnet.stellar.org".to_string()),
        network_passphrase: std::env::var("SOROBAN_NETWORK_PASSPHRASE")
            .unwrap_or_else(|_| "Test SDF Network ; September 2015".to_string()),
    };

    let donation_contract_id =
        std::env::var("DONATION_CONTRACT_ID").unwrap_or_else(|_| String::new());

    let rate_limiter = Arc::new(RateLimiter::new(
        RateLimit::new(
            env_u64("RATE_LIMIT_MAX", 120) as u32,
            Duration::from_secs(env_u64("RATE_LIMIT_WINDOW_SECS", 60)),
        ),
        build_route_limits(),
    ));

    let state = Arc::new(AppState {
        network_config,
        donation_contract_id,
        webhook_manager: WebhookManager::new(),
        metrics: Arc::new(RwLock::new(MetricsData::default())),
        startup_time: Instant::now(),
        http_metrics: Arc::new(HttpMetrics::new()),
        rate_limiter: rate_limiter.clone(),
    });

    // Evict expired rate-limit windows so the table cannot grow without bound.
    {
        let limiter = rate_limiter.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(60));
            loop {
                ticker.tick().await;
                limiter.prune();
            }
        });
    }

    let request_timeout = Duration::from_secs(env_u64("REQUEST_TIMEOUT_SECS", 30));

    let app = Router::new()
        .route("/health", get(health))
        .route("/ready", get(readiness))
        .route("/metrics", get(metrics_endpoint))
        .route("/api/donations/submit", post(submit_donation))
        .route("/api/donations/{tx_hash}", get(get_donation))
        .layer(TimeoutLayer::new(request_timeout))
        .layer(middleware::from_fn_with_state(state.clone(), rate_limit))
        .layer(middleware::from_fn_with_state(state.clone(), track_metrics))
        .layer(middleware::from_fn(correlate))
        .with_state(state);

    let bind = std::env::var("BIND_ADDRESS").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
    info!(bind = %bind, "listening");
    let listener = tokio::net::TcpListener::bind(&bind).await.unwrap();

    let shutdown_timeout = Duration::from_secs(env_u64("SHUTDOWN_TIMEOUT_SECS", 15));
    let shutdown_complete = Arc::new(AtomicBool::new(false));

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(shutdown_complete.clone(), shutdown_timeout))
    .await
    .unwrap();

    shutdown_complete.store(true, Ordering::SeqCst);
    info!("StellarAid worker stopped");
}
