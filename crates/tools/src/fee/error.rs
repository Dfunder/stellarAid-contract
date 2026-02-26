use std::fmt;

/// Fee estimation error types
#[derive(Debug, Clone)]
pub enum FeeError {
    /// Unable to fetch base fee from Horizon
    HorizonUnavailable(String),
    /// Invalid fee retrieved from network
    InvalidFeeValue(String),
    /// Currency conversion failed
    CurrencyConversionFailed(String),
    /// Invalid currency code
    InvalidCurrency(String),
    /// Cache expired or unavailable
    CacheUnavailable(String),
    /// Invalid operation count
    InvalidOperationCount(String),
    /// Network error while fetching fees
    NetworkError(String),
    /// Parsing error from Horizon response
    ParseError(String),
    /// Invalid configuration
    InvalidConfig(String),
    /// Timeout while fetching fees
    Timeout,
    /// Unknown error
    Other(String),
}

impl fmt::Display for FeeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FeeError::HorizonUnavailable(msg) => write!(f, "Horizon unavailable: {}", msg),
            FeeError::InvalidFeeValue(msg) => write!(f, "Invalid fee value: {}", msg),
            FeeError::CurrencyConversionFailed(msg) => {
                write!(f, "Currency conversion failed: {}", msg)
            }
            FeeError::InvalidCurrency(code) => write!(f, "Invalid currency: {}", code),
            FeeError::CacheUnavailable(msg) => write!(f, "Cache unavailable: {}", msg),
            FeeError::InvalidOperationCount(msg) => write!(f, "Invalid operation count: {}", msg),
            FeeError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            FeeError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            FeeError::InvalidConfig(msg) => write!(f, "Invalid configuration: {}", msg),
            FeeError::Timeout => write!(f, "Timeout while fetching fees"),
            FeeError::Other(msg) => write!(f, "Fee error: {}", msg),
        }
    }
}

impl std::error::Error for FeeError {}

pub type FeeResult<T> = Result<T, FeeError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let error = FeeError::HorizonUnavailable("connection refused".to_string());
        assert_eq!(
            error.to_string(),
            "Horizon unavailable: connection refused"
        );
    }

    #[test]
    fn test_invalid_fee_display() {
        let error = FeeError::InvalidFeeValue("negative fee".to_string());
        assert_eq!(error.to_string(), "Invalid fee value: negative fee");
    }

    #[test]
    fn test_timeout_display() {
        let error = FeeError::Timeout;
        assert_eq!(error.to_string(), "Timeout while fetching fees");
    }

    #[test]
    fn test_currency_conversion_error() {
        let error = FeeError::CurrencyConversionFailed("BTC rate unavailable".to_string());
        assert_eq!(
            error.to_string(),
            "Currency conversion failed: BTC rate unavailable"
        );
    }

    #[test]
    fn test_invalid_currency_error() {
        let error = FeeError::InvalidCurrency("XYZ".to_string());
        assert_eq!(error.to_string(), "Invalid currency: XYZ");
    }
}
