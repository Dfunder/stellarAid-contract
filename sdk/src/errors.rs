use thiserror::Error;

/// Top-level error type for the StellarAid blockchain integration layer.
#[derive(Debug, Error)]
pub enum StellarAidError {
    #[error("Horizon API error: {0}")]
    HorizonError(String),

    #[error("Soroban RPC error: {0}")]
    SorobanError(String),

    #[error("Keypair error: {0}")]
    KeypairError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Transaction failed: {0}")]
    TransactionFailed(String),

    #[error("Contract error: {0}")]
    ContractError(String),

    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("Wallet connection error: {0}")]
    WalletConnection(String),

    #[error("Compression error: {0}")]
    Compression(String),

    #[error("Batch error: {0}")]
    Batch(String),
}

impl StellarAidError {
    pub fn horizon(msg: impl Into<String>) -> Self {
        Self::HorizonError(msg.into())
    }

    pub fn soroban(msg: impl Into<String>) -> Self {
        Self::SorobanError(msg.into())
    }

    pub fn keypair(msg: impl Into<String>) -> Self {
        Self::KeypairError(msg.into())
    }

    pub fn validation(msg: impl Into<String>) -> Self {
        Self::ValidationError(msg.into())
    }

    pub fn tx_failed(msg: impl Into<String>) -> Self {
        Self::TransactionFailed(msg.into())
    }

    pub fn contract(msg: impl Into<String>) -> Self {
        Self::ContractError(msg.into())
    }

    pub fn compression(msg: impl Into<String>) -> Self {
        Self::Compression(msg.into())
    }

    pub fn batch(msg: impl Into<String>) -> Self {
        Self::Batch(msg.into())
    }
}

pub type Result<T> = std::result::Result<T, StellarAidError>;