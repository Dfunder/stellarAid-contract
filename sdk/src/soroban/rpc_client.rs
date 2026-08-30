use crate::compression::{self, CompressionFormat, CompressionStats};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;

#[derive(Debug, Error)]
pub enum RpcError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("RPC error: {0}")]
    Rpc(String),
    #[error("Unexpected status: {0}")]
    UnexpectedStatus(String),
}

#[derive(Debug, Deserialize)]
pub struct SimulationResult {
    pub cost: Option<serde_json::Value>,
    pub results: Option<Vec<serde_json::Value>>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SendResult {
    pub hash: String,
    pub status: String,
}

#[derive(Debug, PartialEq)]
pub enum TransactionStatus {
    Pending,
    Success,
    Failed,
    NotFound,
}

#[derive(Debug, Deserialize)]
struct RpcResponse<T> {
    result: Option<T>,
    error: Option<RpcErrorObj>,
}

#[derive(Debug, Deserialize)]
struct RpcErrorObj {
    message: String,
}

#[derive(Debug, Serialize)]
struct RpcRequest<'a> {
    jsonrpc: &'a str,
    id: u32,
    method: &'a str,
    params: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct TxStatusResult {
    status: String,
}

pub struct SorobanRpcClient {
    client: Client,
    rpc_url: String,
    /// Default compression format for query results.
    compression_format: CompressionFormat,
    /// Shared compression statistics.
    compression_stats: Arc<Mutex<CompressionStats>>,
}

impl SorobanRpcClient {
    pub fn new(rpc_url: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            rpc_url: rpc_url.into(),
            compression_format: CompressionFormat::None,
            compression_stats: Arc::new(Mutex::new(CompressionStats::default())),
        }
    }

    /// Create a new client with a specific compression format for query results.
    pub fn with_compression(
        rpc_url: impl Into<String>,
        format: CompressionFormat,
    ) -> Self {
        Self {
            client: Client::new(),
            rpc_url: rpc_url.into(),
            compression_format: format,
            compression_stats: Arc::new(Mutex::new(CompressionStats::default())),
        }
    }

    /// Set the default compression format.
    pub fn set_compression(&mut self, format: CompressionFormat) {
        self.compression_format = format;
    }

    /// Get the current compression format.
    pub fn compression_format(&self) -> CompressionFormat {
        self.compression_format
    }

    /// Get a snapshot of compression statistics.
    pub async fn compression_stats(&self) -> CompressionStats {
        self.compression_stats.lock().await.clone()
    }

    /// Compress query result bytes using the client's configured format.
    pub async fn compress_response(&self, data: &[u8]) -> Result<Vec<u8>, RpcError> {
        let mut stats = self.compression_stats.lock().await;
        compression::compress(data, self.compression_format, Some(&mut stats))
            .map_err(|e| RpcError::Rpc(format!("compression failed: {}", e)))
    }

    /// Decompress query result bytes using the client's configured format.
    pub async fn decompress_response(&self, data: &[u8]) -> Result<Vec<u8>, RpcError> {
        let mut stats = self.compression_stats.lock().await;
        compression::decompress(data, self.compression_format, Some(&mut stats))
            .map_err(|e| RpcError::Rpc(format!("decompression failed: {}", e)))
    }

    async fn call<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<T, RpcError> {
        let req = RpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method,
            params,
        };
        let resp = self
            .client
            .post(&self.rpc_url)
            .json(&req)
            .send()
            .await?
            .json::<RpcResponse<T>>()
            .await?;

        if let Some(err) = resp.error {
            return Err(RpcError::Rpc(err.message));
        }
        resp.result.ok_or_else(|| RpcError::Rpc("Empty result".into()))
    }

    #[tracing::instrument(skip(self), fields(xdr = %xdr))]
    pub async fn simulate_transaction(&self, xdr: &str) -> Result<SimulationResult, RpcError> {
        self.call(
            "simulateTransaction",
            serde_json::json!({ "transaction": xdr }),
        )
        .await
    }

    #[tracing::instrument(skip(self), fields(xdr = %xdr))]
    pub async fn send_transaction(&self, xdr: &str) -> Result<SendResult, RpcError> {
        self.call(
            "sendTransaction",
            serde_json::json!({ "transaction": xdr }),
        )
        .await
    }

    /// Health check: calls getHealth RPC. Returns success if the node responds.
    pub async fn get_health(&self) -> Result<(), RpcError> {
        let _: serde_json::Value = self.call("getHealth", serde_json::json!({})).await?;
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(hash))]
    pub async fn get_transaction_status(&self, hash: &str) -> Result<TransactionStatus, RpcError> {
        let result: TxStatusResult = self
            .call("getTransaction", serde_json::json!({ "hash": hash }))
            .await?;

        Ok(match result.status.as_str() {
            "PENDING" => TransactionStatus::Pending,
            "SUCCESS" => TransactionStatus::Success,
            "FAILED" => TransactionStatus::Failed,
            "NOT_FOUND" => TransactionStatus::NotFound,
            other => return Err(RpcError::UnexpectedStatus(other.to_string())),
        })
    }
}