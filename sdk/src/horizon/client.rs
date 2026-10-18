use crate::errors::StellarAidError;
use crate::health::{EndpointHealth, HEALTH_PROBE_TIMEOUT};
use reqwest::Client;
use serde::Deserialize;
use std::time::Instant;
use thiserror::Error;

#[derive(Debug, Default)]
pub struct HorizonClient {
    client: Client,
    base_url: String,
}

#[derive(Debug, Error)]
pub enum HorizonError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Horizon API error: {0}")]
    Api(String),
}

#[derive(Debug, Deserialize)]
pub struct AccountResponse {
    pub id: String,
    pub sequence: String,
    pub balances: Vec<Balance>,
}

#[derive(Debug, Deserialize)]
pub struct Balance {
    pub balance: String,
    pub asset_type: String,
    #[serde(default)]
    pub asset_code: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TransactionPage {
    pub _embedded: TransactionEmbedded,
}

#[derive(Debug, Deserialize)]
pub struct TransactionEmbedded {
    pub records: Vec<TransactionRecord>,
}

#[derive(Debug, Deserialize)]
pub struct TransactionRecord {
    pub hash: String,
    pub created_at: String,
    pub successful: bool,
}

#[derive(Debug, Deserialize)]
pub struct PaymentPage {
    pub _embedded: PaymentEmbedded,
}

#[derive(Debug, Deserialize)]
pub struct PaymentEmbedded {
    pub records: Vec<PaymentRecord>,
}

#[derive(Debug, Deserialize)]
pub struct PaymentRecord {
    pub id: String,
    #[serde(rename = "type")]
    pub payment_type: String,
    #[serde(default)]
    pub amount: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TransactionDetail {
    pub hash: String,
    pub created_at: String,
    pub successful: bool,
    pub envelope_xdr: String,
    #[serde(default)]
    pub ledger: Option<u64>,
}

impl HorizonClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.into(),
        }
    }

    #[tracing::instrument(skip(self), fields(address))]
    pub async fn get_account(&self, address: &str) -> Result<AccountResponse, HorizonError> {
        let url = format!("{}/accounts/{}", self.base_url, address);
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(HorizonError::Api(resp.text().await.unwrap_or_default()));
        }
        Ok(resp.json().await?)
    }

    #[tracing::instrument(skip(self), fields(address, cursor = ?cursor))]
    pub async fn get_transactions(
        &self,
        address: &str,
        cursor: Option<&str>,
    ) -> Result<TransactionPage, HorizonError> {
        let mut url = format!("{}/accounts/{}/transactions?order=desc&limit=50", self.base_url, address);
        if let Some(c) = cursor {
            url.push_str(&format!("&cursor={}", c));
        }
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(HorizonError::Api(resp.text().await.unwrap_or_default()));
        }
        Ok(resp.json().await?)
    }

    #[tracing::instrument(skip(self), fields(address, cursor = ?cursor))]
    pub async fn get_payments(
        &self,
        address: &str,
        cursor: Option<&str>,
    ) -> Result<PaymentPage, HorizonError> {
        let mut url = format!("{}/accounts/{}/payments?order=desc&limit=50", self.base_url, address);
        if let Some(c) = cursor {
            url.push_str(&format!("&cursor={}", c));
        }
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(HorizonError::Api(resp.text().await.unwrap_or_default()));
        }
        Ok(resp.json().await?)
    }

    #[tracing::instrument(skip(self), fields(hash))]
    pub async fn get_transaction(&self, hash: &str) -> Result<TransactionDetail, HorizonError> {
        let url = format!("{}/transactions/{}", self.base_url, hash);
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(HorizonError::Api(resp.text().await.unwrap_or_default()));
        }
        Ok(resp.json().await?)
    }

    /// Typed liveness probe (#980): hits Horizon's root endpoint.
    ///
    /// Returns `Ok(EndpointHealth)` when Horizon answers (with `healthy`
    /// reflecting the status), `Err(BadRequest)` on a 4xx — almost always a
    /// wrong URL — and `Err(EndpointUnreachable | RequestTimeout)` when it does
    /// not answer within [`HEALTH_PROBE_TIMEOUT`]. Bounded by that short timeout
    /// so a probe can never hang.
    pub async fn health(&self) -> Result<EndpointHealth, StellarAidError> {
        let url = format!("{}/", self.base_url.trim_end_matches('/'));
        let started = Instant::now();
        let probe = tokio::time::timeout(HEALTH_PROBE_TIMEOUT, self.client.get(&url).send()).await;
        let elapsed = started.elapsed().as_millis();

        match probe {
            Err(_) => Err(StellarAidError::request_timeout(format!(
                "Horizon health probe to {url} exceeded {HEALTH_PROBE_TIMEOUT:?}"
            ))),
            Ok(Err(e)) => Err(StellarAidError::from_reqwest(e)),
            Ok(Ok(resp)) => {
                let status = resp.status();
                if status.is_success() {
                    Ok(EndpointHealth::up(url, elapsed))
                } else if status.is_client_error() {
                    Err(StellarAidError::from_http_status(
                        status.as_u16(),
                        status.canonical_reason().unwrap_or("client error"),
                    ))
                } else {
                    Ok(EndpointHealth::down(url, format!("HTTP {status}"), elapsed))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn health_rejects_a_malformed_url_without_retrying() {
        // A relative URL fails at request build; this must be a clear,
        // non-retryable error rather than a hang.
        let client = HorizonClient::new("not-a-valid-url");
        let err = client.health().await.unwrap_err();
        assert!(matches!(
            err,
            StellarAidError::BadRequest(_) | StellarAidError::EndpointUnreachable(_)
        ));
        assert!(!matches!(err, StellarAidError::RequestTimeout(_)));
    }
}
