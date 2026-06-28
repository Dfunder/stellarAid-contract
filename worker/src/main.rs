mod webhooks;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use sdk::{
    errors::StellarAidError,
    logging,
    retry::{retry_async, RetryConfig},
    soroban::rpc_client::SorobanRpcClient,
    transaction_builder::{build_donate_transaction_full, DonationParams, NetworkConfig},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;
use webhooks::{WebhookManager, WebhookPayload};

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

#[derive(Clone)]
pub struct AppState {
    pub network_config: NetworkConfig,
    pub donation_contract_id: String,
    pub webhook_manager: WebhookManager,
}

async fn submit_donation(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SubmitDonationRequest>,
) -> Result<Json<SubmitDonationResponse>, (StatusCode, Json<ErrorResponse>)> {
    if req.amount <= 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "amount must be positive".to_string(),
            }),
        ));
    }

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

    let xdr = retry_async(&retry_config, || async {
        build_donate_transaction_full(&params, &network)
            .await
            .map_err(|e| StellarAidError::SorobanError(e.to_string()))
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("transaction build failed: {}", e),
            }),
        )
    })?;

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

    let status = retry_async(&RetryConfig::default(), || async {
        rpc.get_transaction_status(&tx_hash)
            .await
            .map_err(|e| StellarAidError::SorobanError(e.to_string()))
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("failed to get transaction status: {}", e),
            }),
        )
    })?;

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

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
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

    let state = Arc::new(AppState {
        network_config,
        donation_contract_id,
        webhook_manager: WebhookManager::new(),
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/donations/submit", post(submit_donation))
        .route("/api/donations/{tx_hash}", get(get_donation))
        .with_state(state);

    let bind = std::env::var("BIND_ADDRESS").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
    info!(bind = %bind, "listening");
    let listener = tokio::net::TcpListener::bind(&bind).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
