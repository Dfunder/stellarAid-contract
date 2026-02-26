//! Horizon API Client Errors
//!
//! Comprehensive error types for Horizon client operations.

use thiserror::Error;
use std::time::Duration;

/// Errors that can occur during Horizon API interactions
#[derive(Error, Debug)]
pub enum HorizonError {
    /// Network connectivity error
    #[error("Network error: {0}")]
    NetworkError(String),

    /// HTTP request failed
    #[error("HTTP request failed with status {status}: {message}")]
    HttpError { status: u16, message: String },

    /// Request timeout
    #[error("Request timeout after {duration:?}")]
    Timeout { duration: Duration },

    /// Rate limit exceeded
    #[error("Rate limit exceeded. Retry after {retry_after:?}")]
    RateLimited { retry_after: Duration },

    /// Invalid request
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Invalid response format
    #[error("Invalid response format: {0}")]
    InvalidResponse(String),

    /// Server error (5xx)
    #[error("Server error ({status}): {message}")]
    ServerError { status: u16, message: String },

    /// Not found error (404)
    #[error("Resource not found: {0}")]
    NotFound(String),

    /// Bad request error (400)
    #[error("Bad request: {0}")]
    BadRequest(String),

    /// Unauthorized error (401)
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// Forbidden error (403)
    #[error("Forbidden: {0}")]
    Forbidden(String),

    /// Connection refused
    #[error("Connection refused: {0}")]
    ConnectionRefused(String),

    /// Connection reset
    #[error("Connection reset: {0}")]
    ConnectionReset(String),

    /// DNS resolution failed
    #[error("DNS resolution failed: {0}")]
    DnsError(String),

    /// TLS error
    #[error("TLS error: {0}")]
    TlsError(String),

    /// Horizon service unavailable
    #[error("Horizon service unavailable: {0}")]
    ServiceUnavailable(String),

    /// Cache error
    #[error("Cache error: {0}")]
    CacheError(String),

    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// JSON parsing error
    #[error("JSON parsing error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// URL parsing error
    #[error("URL parsing error: {0}")]
    UrlError(#[from] url::ParseError),

    /// Other errors
    #[error("Error: {0}")]
    Other(String),
}

impl HorizonError {
    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            HorizonError::NetworkError(_)
                | HorizonError::Timeout { .. }
                | HorizonError::RateLimited { .. }
                | HorizonError::ConnectionRefused(_)
                | HorizonError::ConnectionReset(_)
                | HorizonError::ServerError { .. }
                | HorizonError::ServiceUnavailable(_)
                | HorizonError::DnsError(_)
        )
    }

    /// Check if this is a rate limit error
    pub fn is_rate_limited(&self) -> bool {
        matches!(self, HorizonError::RateLimited { .. })
    }

    /// Check if this is a server error (5xx)
    pub fn is_server_error(&self) -> bool {
        matches!(
            self,
            HorizonError::ServerError { .. } | HorizonError::ServiceUnavailable(_)
        )
    }

    /// Check if this is a client error (4xx)
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            HorizonError::InvalidRequest(_)
                | HorizonError::BadRequest(_)
                | HorizonError::Unauthorized(_)
                | HorizonError::Forbidden(_)
                | HorizonError::NotFound(_)
        )
    }

    /// Get suggested retry duration if available
    pub fn suggested_retry_duration(&self) -> Option<Duration> {
        match self {
            HorizonError::RateLimited { retry_after } => Some(*retry_after),
            HorizonError::ServerError { .. } => Some(Duration::from_secs(5)),
            HorizonError::ServiceUnavailable(_) => Some(Duration::from_secs(10)),
            HorizonError::Timeout { .. } => Some(Duration::from_secs(2)),
            _ => None,
        }
    }

    /// Convert reqwest error to HorizonError
    pub fn from_reqwest(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            HorizonError::Timeout {
                duration: Duration::from_secs(30),
            }
        } else if err.is_connect() {
            HorizonError::ConnectionRefused(err.to_string())
        } else if err.is_request() {
            HorizonError::NetworkError(err.to_string())
        } else if let Some(status) = err.status() {
            match status {
                reqwest::http::StatusCode::NOT_FOUND => {
                    HorizonError::NotFound(err.to_string())
                }
                reqwest::http::StatusCode::BAD_REQUEST => {
                    HorizonError::BadRequest(err.to_string())
                }
                reqwest::http::StatusCode::UNAUTHORIZED => {
                    HorizonError::Unauthorized(err.to_string())
                }
                reqwest::http::StatusCode::FORBIDDEN => {
                    HorizonError::Forbidden(err.to_string())
                }
                _ if status.is_server_error() => HorizonError::ServerError {
                    status: status.as_u16(),
                    message: err.to_string(),
                },
                _ => HorizonError::HttpError {
                    status: status.as_u16(),
                    message: err.to_string(),
                },
            }
        } else {
            HorizonError::NetworkError(err.to_string())
        }
    }
}

/// Result type for Horizon operations
pub type HorizonResult<T> = Result<T, HorizonError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_retryable() {
        let network_err = HorizonError::NetworkError("connection failed".to_string());
        assert!(network_err.is_retryable());

        let not_found = HorizonError::NotFound("resource not found".to_string());
        assert!(!not_found.is_retryable());
    }

    #[test]
    fn test_is_server_error() {
        let server_err = HorizonError::ServerError {
            status: 500,
            message: "internal error".to_string(),
        };
        assert!(server_err.is_server_error());

        let client_err = HorizonError::BadRequest("invalid".to_string());
        assert!(!client_err.is_server_error());
    }

    #[test]
    fn test_is_client_error() {
        let bad_request = HorizonError::BadRequest("invalid".to_string());
        assert!(bad_request.is_client_error());

        let server_err = HorizonError::ServerError {
            status: 500,
            message: "error".to_string(),
        };
        assert!(!server_err.is_client_error());
    }

    #[test]
    fn test_suggested_retry_duration() {
        let rate_limited = HorizonError::RateLimited {
            retry_after: Duration::from_secs(60),
        };
        assert_eq!(
            rate_limited.suggested_retry_duration(),
            Some(Duration::from_secs(60))
        );

        let not_found = HorizonError::NotFound("not found".to_string());
        assert_eq!(not_found.suggested_retry_duration(), None);
    }
}
