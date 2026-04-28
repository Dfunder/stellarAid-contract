use anyhow::{Result, Context, anyhow};
use anyhow::{Result, Context, anyhow};
use serde::{Serialize, Deserialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::env;

/// Represents a signing request for a Stellar transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigningRequest {
    pub id: String,
    pub network: String,
    pub transaction_xdr: String,
    pub description: String,
    pub created_at: u64,
}

/// Builder for creating signing requests
pub struct SigningRequestBuilder {
    id: String,
    network: String,
    transaction_xdr: String,
    description: String,
    created_at: u64,
}

impl SigningRequestBuilder {
    /// Create a new signing request builder
    pub fn new(transaction_xdr: String, network: Option<String>) -> Result<Self> {
        let network = network.unwrap_or_else(|| {
            env::var("SOROBAN_NETWORK").unwrap_or_else(|_| "testnet".to_string())
        });

        let id = format!(
            "req_{}",
            chrono::Local::now().timestamp_millis()
        );

        Ok(SigningRequestBuilder {
            id,
            network,
            transaction_xdr,
            description: String::new(),
            created_at: chrono::Local::now().timestamp() as u64,
        })
    }

    /// Set description for the signing request
    pub fn with_description(mut self, description: String) -> Self {
        self.description = description;
        self
    }

    /// Build the signing request
    pub fn build(self) -> Result<SigningRequest> {
        if self.transaction_xdr.is_empty() {
            return Err(anyhow!("Transaction XDR cannot be empty"));
        }

        Ok(SigningRequest {
            id: self.id,
            network: self.network,
            transaction_xdr: self.transaction_xdr,
            description: self.description,
            created_at: self.created_at,
        })
    }
}

/// Helper for building common transaction types
pub struct TransactionBuilder;

impl TransactionBuilder {
    /// Build a donation transaction signing request
    pub fn build_donation_request(
        donor_address: String,
        campaign_id: u64,
        amount: i128,
        asset: String,
        memo: Option<String>,
    ) -> Result<SigningRequest> {
        let desc = format!(
            "Donate {} {} to campaign #{}",
            amount, asset, campaign_id
        );

        // Placeholder XDR - in real implementation, this would be built from actual transaction
        let transaction_xdr = format!(
            "AAAAAA=={}{}{}",
            donor_address, campaign_id, amount
        );

        let mut builder = SigningRequestBuilder::new(transaction_xdr, None)?
            .with_description(desc);

        if let Some(m) = memo {
            builder = builder.with_description(format!("{} [memo: {}]", builder.description, m));
        }

        builder.build()
    }

    /// Build a campaign creation transaction signing request
    pub fn build_campaign_request(
        creator_address: String,
        title: String,
        goal: i128,
        deadline: u64,
    ) -> Result<SigningRequest> {
        let desc = format!(
            "Create campaign '{}' with goal {} until {}",
            title, goal, deadline
        );

        let transaction_xdr = format!(
            "AAAAAA=={}{}{}{}",
            creator_address, title, goal, deadline
        );

        SigningRequestBuilder::new(transaction_xdr, None)?
            .with_description(desc)
            .build()
    }
}

impl SigningRequest {
    /// Convert signing request to JSON for transmission
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .context("Failed to serialize signing request to JSON")
    }

    /// Create from JSON string
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json)
            .context("Failed to deserialize signing request from JSON")
    }

    /// Convert to wallet signing format (for Freighter and similar)
    pub fn to_wallet_format(&self) -> Result<String> {
        let wallet_request = json!({
            "id": self.id,
            "type": "tx",
            "xdr": self.transaction_xdr,
            "network": self.network,
            "description": self.description,
            "timestamp": self.created_at,
        });

        Ok(wallet_request.to_string())
    }

    /// Display request details
    pub fn display(&self) {
        println!("📝 Signing Request");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("ID:          {}", self.id);
        println!("Network:     {}", self.network);
        println!("Description: {}", self.description);
        println!("Created:     {}", self.created_at);
        println!();
        println!("Transaction XDR:");
        println!("{}", self.transaction_xdr);
    }

    /// Validate the signing request
    pub fn validate(&self) -> Result<()> {
        if self.id.is_empty() {
            return Err(anyhow!("Request ID cannot be empty"));
        }

        if self.transaction_xdr.is_empty() {
            return Err(anyhow!("Transaction XDR cannot be empty"));
        }

        match self.network.as_str() {
            "testnet" | "mainnet" | "public" => Ok(()),
            _ => Err(anyhow!("Invalid network: {}", self.network)),
        }
    }

    /// Get QR code data for mobile wallet
    pub fn to_qr_data(&self) -> Result<String> {
        self.to_wallet_format()
    }
}

/// Issue #132 – A transaction that has been signed server-side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSignedTransaction {
    pub request_id: String,
    pub transaction_xdr: String,
    /// Hex-encoded SHA-256(secret_key || transaction_xdr) used as the server signature.
    pub signature: String,
    pub signed_at: i64,
}

impl SigningRequest {
    /// Issue #132 – Sign this transaction server-side using the provided secret key.
    ///
    /// Produces a deterministic signature: SHA-256(secret_key_bytes || xdr_bytes),
    /// which proves the server holding the secret key authorised this XDR.
    pub fn sign_server_side(&self, secret_key: &str) -> Result<ServerSignedTransaction> {
        if secret_key.is_empty() {
            return Err(anyhow!("Secret key must not be empty"));
        }
        crate::key_manager::KeyManager::validate_secret_key(secret_key)?;

        let mut hasher = Sha256::new();
        hasher.update(secret_key.as_bytes());
        hasher.update(self.transaction_xdr.as_bytes());
        let signature = hex::encode(hasher.finalize());

        Ok(ServerSignedTransaction {
            request_id: self.id.clone(),
            transaction_xdr: self.transaction_xdr.clone(),
            signature,
            signed_at: chrono::Local::now().timestamp(),
        })
    }

    /// Issue #132 – Sign using the secret key stored in the SOROBAN_SECRET_KEY env var.
    pub fn sign_from_env(&self) -> Result<ServerSignedTransaction> {
        let secret_key = env::var("SOROBAN_SECRET_KEY")
            .context("SOROBAN_SECRET_KEY not set in environment")?;
        self.sign_server_side(&secret_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signing_request_builder() {
        let xdr = "AAAAAA==test".to_string();
        let req = SigningRequestBuilder::new(xdr, Some("testnet".to_string()))
            .unwrap()
            .with_description("Test donation".to_string())
            .build();

        assert!(req.is_ok());
        let req = req.unwrap();
        assert!(req.id.starts_with("req_"));
        assert_eq!(req.network, "testnet");
        assert_eq!(req.description, "Test donation");
    }

    #[test]
    fn test_signing_request_validation() {
        let req = SigningRequest {
            id: "req_123".to_string(),
            network: "testnet".to_string(),
            transaction_xdr: "AAAAAA==".to_string(),
            description: "Test".to_string(),
            created_at: 0,
        };

        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_signing_request_json() {
        let req = SigningRequest {
            id: "req_123".to_string(),
            network: "testnet".to_string(),
            transaction_xdr: "AAAAAA==".to_string(),
            description: "Test".to_string(),
            created_at: 0,
        };

        let json = req.to_json().unwrap();
        let restored = SigningRequest::from_json(&json).unwrap();
        assert_eq!(restored.id, req.id);
    }

    #[test]
    fn test_sign_server_side_produces_signature() {
        let req = SigningRequest {
            id: "req_123".to_string(),
            network: "testnet".to_string(),
            transaction_xdr: "AAAAAA==test_xdr".to_string(),
            description: "Test".to_string(),
            created_at: 0,
        };
        let secret = "SBZXVMIRWXL5VZVKXWV2FGKYTQ5VV5VRNJYQVZKYWW3XYVYP3IXGKDU";
        let signed = req.sign_server_side(secret).unwrap();
        assert_eq!(signed.request_id, "req_123");
        assert!(!signed.signature.is_empty());
        assert_eq!(signed.transaction_xdr, req.transaction_xdr);
    }

    #[test]
    fn test_sign_server_side_is_deterministic() {
        let req = SigningRequest {
            id: "req_123".to_string(),
            network: "testnet".to_string(),
            transaction_xdr: "AAAAAA==test_xdr".to_string(),
            description: "Test".to_string(),
            created_at: 0,
        };
        let secret = "SBZXVMIRWXL5VZVKXWV2FGKYTQ5VV5VRNJYQVZKYWW3XYVYP3IXGKDU";
        let sig1 = req.sign_server_side(secret).unwrap().signature;
        let sig2 = req.sign_server_side(secret).unwrap().signature;
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn test_sign_server_side_different_keys_differ() {
        let req = SigningRequest {
            id: "req_123".to_string(),
            network: "testnet".to_string(),
            transaction_xdr: "AAAAAA==test_xdr".to_string(),
            description: "Test".to_string(),
            created_at: 0,
        };
        let sig1 = req.sign_server_side("SBZXVMIRWXL5VZVKXWV2FGKYTQ5VV5VRNJYQVZKYWW3XYVYP3IXGKDU").unwrap().signature;
        let sig2 = req.sign_server_side("SCZANGBA5QDPSBM5QOTSXSI7JKEFYABMUQRPTGMWNJKFA5ENDNSQSTE").unwrap().signature;
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_sign_server_side_rejects_invalid_key() {
        let req = SigningRequest {
            id: "req_123".to_string(),
            network: "testnet".to_string(),
            transaction_xdr: "AAAAAA==".to_string(),
            description: "Test".to_string(),
            created_at: 0,
        };
        assert!(req.sign_server_side("not_a_valid_key").is_err());
        assert!(req.sign_server_side("").is_err());
    }
}
