use std::time::Duration;

use log::info;
use serde::{Deserialize, Serialize};

use super::error::{FeeError, FeeResult};

/// Horizon base fee response
#[derive(Debug, Clone, Serialize, Deserialize)]
struct HorizonLedgerResponse {
    /// Base fee in stroops
    base_fee_rate: Option<i64>,
    /// Base reserve in stroops
    base_reserve: Option<String>,
}

/// Horizon fee fetcher
pub struct HorizonFeeFetcher {
    server_url: String,
    timeout_secs: u64,
}

impl HorizonFeeFetcher {
    /// Create new Horizon fee fetcher
    pub fn new(server_url: String) -> Self {
        Self {
            server_url,
            timeout_secs: 30,
        }
    }

    /// Create fetcher for public Horizon
    pub fn public_horizon() -> Self {
        Self::new("https://horizon.stellar.org".to_string())
    }

    /// Set timeout (seconds)
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    /// Fetch current base fee from Horizon
    pub async fn fetch_base_fee(&self) -> FeeResult<i64> {
        let url = format!("{}/ledgers?sort=desc&limit=1", self.server_url);

        info!("Fetching base fee from Horizon: {}", url);

        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .timeout(Duration::from_secs(self.timeout_secs))
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    FeeError::Timeout
                } else if e.is_connect() {
                    FeeError::HorizonUnavailable("Connection failed".to_string())
                } else {
                    FeeError::NetworkError(e.to_string())
                }
            })?;

        if !response.status().is_success() {
            return Err(FeeError::HorizonUnavailable(format!(
                "HTTP status: {}",
                response.status()
            )));
        }

        let body = response.text().await.map_err(|e| {
            FeeError::ParseError(format!("Failed to read response body: {}", e))
        })?;

        self.parse_base_fee(&body)
    }

    /// Parse base fee from Horizon response
    fn parse_base_fee(&self, response_body: &str) -> FeeResult<i64> {
        // Parse JSON response
        let parsed = serde_json::from_str::<serde_json::Value>(response_body)
            .map_err(|e| FeeError::ParseError(e.to_string()))?;

        // Extract ledgers array
        let records = parsed
            .get("_embedded")
            .and_then(|e| e.get("records"))
            .and_then(|r| r.as_array())
            .and_then(|arr| arr.first())
            .ok_or_else(|| {
                FeeError::ParseError("No ledger records in response".to_string())
            })?;

        // Extract base_fee_rate
        let base_fee = records
            .get("base_fee_rate")
            .and_then(|f| f.as_i64())
            .ok_or_else(|| {
                FeeError::ParseError("base_fee_rate field missing or invalid".to_string())
            })?;

        if base_fee < 0 {
            return Err(FeeError::InvalidFeeValue(
                "base fee is negative".to_string(),
            ));
        }

        if base_fee == 0 {
            return Err(FeeError::InvalidFeeValue(
                "base fee is zero".to_string(),
            ));
        }

        info!("Fetched base fee: {} stroops", base_fee);

        Ok(base_fee)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_horizon_fee_fetcher_creation() {
        let fetcher = HorizonFeeFetcher::new("https://horizon.stellar.org".to_string());
        assert_eq!(fetcher.server_url, "https://horizon.stellar.org");
        assert_eq!(fetcher.timeout_secs, 30);
    }

    #[test]
    fn test_horizon_fee_fetcher_public() {
        let fetcher = HorizonFeeFetcher::public_horizon();
        assert_eq!(fetcher.server_url, "https://horizon.stellar.org");
    }

    #[test]
    fn test_horizon_fee_fetcher_timeout() {
        let fetcher = HorizonFeeFetcher::new("https://horizon.stellar.org".to_string())
            .with_timeout(60);
        assert_eq!(fetcher.timeout_secs, 60);
    }

    #[test]
    fn test_parse_base_fee_valid() {
        let fetcher = HorizonFeeFetcher::public_horizon();
        let response = r#"
        {
            "_embedded": {
                "records": [
                    {
                        "id": "test",
                        "base_fee_rate": 100
                    }
                ]
            }
        }
        "#;

        let fee = fetcher.parse_base_fee(response).unwrap();
        assert_eq!(fee, 100);
    }

    #[test]
    fn test_parse_base_fee_invalid_json() {
        let fetcher = HorizonFeeFetcher::public_horizon();
        let response = "invalid json";

        let result = fetcher.parse_base_fee(response);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_base_fee_missing_field() {
        let fetcher = HorizonFeeFetcher::public_horizon();
        let response = r#"
        {
            "_embedded": {
                "records": []
            }
        }
        "#;

        let result = fetcher.parse_base_fee(response);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_base_fee_negative() {
        let fetcher = HorizonFeeFetcher::public_horizon();
        let response = r#"
        {
            "_embedded": {
                "records": [
                    {
                        "id": "test",
                        "base_fee_rate": -100
                    }
                ]
            }
        }
        "#;

        let result = fetcher.parse_base_fee(response);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_base_fee_zero() {
        let fetcher = HorizonFeeFetcher::public_horizon();
        let response = r#"
        {
            "_embedded": {
                "records": [
                    {
                        "id": "test",
                        "base_fee_rate": 0
                    }
                ]
            }
        }
        "#;

        let result = fetcher.parse_base_fee(response);
        assert!(result.is_err());
    }
}
