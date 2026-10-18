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

    /// The endpoint could not be reached at all (DNS, refused connection, wrong
    /// URL). Retryable (#980).
    #[error("endpoint unreachable: {0}")]
    EndpointUnreachable(String),

    /// The request was accepted but did not complete in time. Retryable (#980).
    #[error("request timed out: {0}")]
    RequestTimeout(String),

    /// The endpoint rejected the request as malformed (4xx). Not retryable
    /// without changing the request (#980).
    #[error("bad request: {0}")]
    BadRequest(String),

    /// The chain rejected the transaction. Not retryable as-is (#980).
    #[error("transaction rejected: {0}")]
    TransactionRejected(String),
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

    pub fn endpoint_unreachable(msg: impl Into<String>) -> Self {
        Self::EndpointUnreachable(msg.into())
    }

    pub fn request_timeout(msg: impl Into<String>) -> Self {
        Self::RequestTimeout(msg.into())
    }

    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }

    pub fn transaction_rejected(msg: impl Into<String>) -> Self {
        Self::TransactionRejected(msg.into())
    }

    /// Classify a transport-level [`reqwest::Error`] into the actionable
    /// variants. A timeout and a refused connection are both retryable, but a
    /// caller usually wants to tell them apart (#980).
    pub fn from_reqwest(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            Self::RequestTimeout(err.to_string())
        } else if err.is_connect() {
            Self::EndpointUnreachable(err.to_string())
        } else if err.is_builder() {
            // A malformed URL or request cannot be fixed by retrying.
            Self::BadRequest(err.to_string())
        } else if err.is_request() {
            Self::EndpointUnreachable(err.to_string())
        } else {
            Self::NetworkError(err)
        }
    }

    /// Classify a non-success HTTP status. 4xx is a malformed request (fix the
    /// request); everything else is treated as transient.
    pub fn from_http_status(status: u16, body: impl Into<String>) -> Self {
        let body = body.into();
        if (400..500).contains(&status) {
            Self::BadRequest(format!("HTTP {status}: {body}"))
        } else {
            Self::RequestTimeout(format!("HTTP {status}: {body}"))
        }
    }

    /// Whether retrying this error could plausibly succeed.
    ///
    /// This is the predicate [`crate::retry::retry_async_classified`] takes, and
    /// the answer to issue #805: `retry_async` used to retry *every* error,
    /// including an unrecoverable validation or rejection failure.
    ///
    /// Retryable: [`StellarAidError::NetworkError`],
    /// [`StellarAidError::EndpointUnreachable`],
    /// [`StellarAidError::RequestTimeout`].
    ///
    /// Not retryable: validation, bad-request, rejected-transaction, contract,
    /// keypair, wallet, compression and batch errors — the server-side
    /// [`StellarAidError::HorizonError`]/[`StellarAidError::SorobanError`]
    /// variants carry a rejection and are likewise not retried.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::NetworkError(_) | Self::EndpointUnreachable(_) | Self::RequestTimeout(_)
        )
    }
}

pub type Result<T> = std::result::Result<T, StellarAidError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_transport_errors_are_retryable() {
        assert!(StellarAidError::endpoint_unreachable("dns").is_retryable());
        assert!(StellarAidError::request_timeout("slow").is_retryable());
        assert!(!StellarAidError::bad_request("HTTP 400").is_retryable());
        assert!(!StellarAidError::transaction_rejected("tx failed").is_retryable());
        assert!(!StellarAidError::validation("amount must be positive").is_retryable());
    }

    #[test]
    fn http_status_is_classified() {
        assert!(matches!(
            StellarAidError::from_http_status(400, "bad"),
            StellarAidError::BadRequest(_)
        ));
        assert!(matches!(
            StellarAidError::from_http_status(404, "missing"),
            StellarAidError::BadRequest(_)
        ));
        // 5xx is transient, not a malformed request.
        assert!(StellarAidError::from_http_status(503, "unavailable").is_retryable());
    }
}