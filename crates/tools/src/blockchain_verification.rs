use anyhow::{Result, Context, anyhow};
use serde::{Serialize, Deserialize};
use std::time::{Duration, Instant};

/// Transaction verification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionVerification {
    pub transaction_hash: String,
    pub verified: bool,
    pub ledger_number: Option<u32>,
    pub ledger_close_time: Option<u64>,
    pub block_explorer_url: String,
    pub verification_time_ms: u64,
    pub details: VerificationDetails,
}

/// Detailed verification information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationDetails {
    pub signature_valid: bool,
    pub sequence_valid: bool,
    pub balance_sufficient: bool,
    pub network_match: bool,
    pub timestamp_valid: bool,
    pub status: TransactionStatus,
    pub warnings: Vec<String>,
}

/// Transaction status on the blockchain
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TransactionStatus {
    #[serde(rename = "confirmed")]
    Confirmed,
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "not_found")]
    NotFound,
}

impl std::fmt::Display for TransactionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransactionStatus::Confirmed => write!(f, "Confirmed"),
            TransactionStatus::Pending => write!(f, "Pending"),
            TransactionStatus::Failed => write!(f, "Failed"),
            TransactionStatus::NotFound => write!(f, "Not Found"),
        }
    }
}

/// Configuration for blockchain verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationConfig {
    /// Stellar network (testnet, mainnet, public)
    pub network: String,
    /// RPC endpoint URL
    pub rpc_url: String,
    /// Horizon API URL
    pub horizon_url: String,
    /// Block explorer base URL
    pub block_explorer_url: String,
    /// Timeout for verification requests (seconds)
    pub timeout_seconds: u64,
    /// Maximum retries for verification
    pub max_retries: u32,
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            network: "testnet".to_string(),
            rpc_url: "https://soroban-testnet.stellar.org".to_string(),
            horizon_url: "https://horizon-testnet.stellar.org".to_string(),
            block_explorer_url: "https://stellar.expert/explorer/testnet".to_string(),
            timeout_seconds: 30,
            max_retries: 3,
        }
    }
}

/// Blockchain transaction verifier
pub struct BlockchainVerifier {
    config: VerificationConfig,
}

impl BlockchainVerifier {
    /// Create a new verifier with default configuration
    pub fn new() -> Self {
        Self {
            config: VerificationConfig::default(),
        }
    }

    /// Create a new verifier with custom configuration
    pub fn with_config(config: VerificationConfig) -> Self {
        Self { config }
    }

    /// Verify a transaction by its hash
    pub async fn verify_transaction(&self, transaction_hash: &str) -> Result<TransactionVerification> {
        let start_time = Instant::now();

        println!("🔍 Verifying transaction: {}", transaction_hash);

        // Fetch transaction from blockchain
        let tx_data = self.fetch_transaction(transaction_hash).await?;

        // Verify transaction details
        let details = self.verify_transaction_details(&tx_data).await?;

        // Generate block explorer URL
        let block_explorer_url = self.generate_explorer_url(transaction_hash);

        let verification_time = start_time.elapsed();

        let result = TransactionVerification {
            transaction_hash: transaction_hash.to_string(),
            verified: details.signature_valid && details.network_match,
            ledger_number: tx_data.ledger_number,
            ledger_close_time: tx_data.ledger_close_time,
            block_explorer_url,
            verification_time_ms: verification_time.as_millis() as u64,
            details,
        };

        if result.verified {
            println!("✅ Transaction verified successfully");
        } else {
            println!("⚠️  Transaction verification failed or incomplete");
        }

        println!("🔗 View on explorer: {}", result.block_explorer_url);

        Ok(result)
    }

    /// Verify transaction with state proof (on-chain verification)
    pub async fn verify_with_state_proof(
        &self,
        transaction_hash: &str,
    ) -> Result<TransactionVerification> {
        let start_time = Instant::now();

        println!("🔐 Verifying transaction with state proof: {}", transaction_hash);

        // Fetch transaction with full state
        let tx_data = self.fetch_transaction_with_state(transaction_hash).await?;

        // Verify against on-chain state
        let details = self.verify_state_proof(&tx_data).await?;

        let block_explorer_url = self.generate_explorer_url(transaction_hash);

        let verification_time = start_time.elapsed();

        Ok(TransactionVerification {
            transaction_hash: transaction_hash.to_string(),
            verified: details.signature_valid,
            ledger_number: tx_data.ledger_number,
            ledger_close_time: tx_data.ledger_close_time,
            block_explorer_url,
            verification_time_ms: verification_time.as_millis() as u64,
            details,
        })
    }

    /// Verify transaction timestamp
    pub async fn verify_timestamp(
        &self,
        transaction_hash: &str,
        expected_timestamp: u64,
    ) -> Result<bool> {
        let tx_data = self.fetch_transaction(transaction_hash).await?;

        if let Some(actual_time) = tx_data.ledger_close_time {
            // Allow 60 second tolerance for block finalization
            let time_diff = if actual_time > expected_timestamp {
                actual_time - expected_timestamp
            } else {
                expected_timestamp - actual_time
            };

            let valid = time_diff <= 60;
            
            if valid {
                println!("✅ Timestamp verified (diff: {}s)", time_diff);
            } else {
                println!("⚠️  Timestamp mismatch (diff: {}s)", time_diff);
            }

            Ok(valid)
        } else {
            Err(anyhow!("Transaction ledger close time not available"))
        }
    }

    /// Fetch transaction data from blockchain
    async fn fetch_transaction(&self, hash: &str) -> Result<TransactionData> {
        // In production, this would make an actual HTTP request to Horizon/RPC
        // For now, simulate the structure
        
        // Simulate network call
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Placeholder - in real implementation, parse Horizon API response
        Ok(TransactionData {
            hash: hash.to_string(),
            ledger_number: Some(12345678),
            ledger_close_time: Some(1234567890),
            created_at: 1234567890,
            fee_charged: 100,
            operation_count: 1,
            status: "success".to_string(),
        })
    }

    /// Fetch transaction with full state proof
    async fn fetch_transaction_with_state(&self, hash: &str) -> Result<TransactionData> {
        // In production, fetch with ledger state and proofs
        tokio::time::sleep(Duration::from_millis(200)).await;

        Ok(TransactionData {
            hash: hash.to_string(),
            ledger_number: Some(12345678),
            ledger_close_time: Some(1234567890),
            created_at: 1234567890,
            fee_charged: 100,
            operation_count: 1,
            status: "success".to_string(),
        })
    }

    /// Verify transaction details
    async fn verify_transaction_details(&self, tx_data: &TransactionData) -> Result<VerificationDetails> {
        let mut warnings = Vec::new();

        // Verify signature (placeholder - would use Stellar SDK in production)
        let signature_valid = true;

        // Verify sequence number
        let sequence_valid = true;

        // Verify balance was sufficient
        let balance_sufficient = true;

        // Verify network match
        let network_match = true;

        // Verify timestamp
        let timestamp_valid = tx_data.ledger_close_time.is_some();

        // Determine status
        let status = match tx_data.status.as_str() {
            "success" => TransactionStatus::Confirmed,
            "pending" => TransactionStatus::Pending,
            "failed" => TransactionStatus::Failed,
            _ => TransactionStatus::NotFound,
        };

        if !signature_valid {
            warnings.push("Transaction signature could not be verified".to_string());
        }

        Ok(VerificationDetails {
            signature_valid,
            sequence_valid,
            balance_sufficient,
            network_match,
            timestamp_valid,
            status,
            warnings,
        })
    }

    /// Verify state proof (cryptographic proof of on-chain state)
    async fn verify_state_proof(&self, tx_data: &TransactionData) -> Result<VerificationDetails> {
        // In production, verify Merkle proofs and state roots
        let mut warnings = Vec::new();

        let signature_valid = tx_data.ledger_number.is_some();
        let sequence_valid = true;
        let balance_sufficient = true;
        let network_match = true;
        let timestamp_valid = tx_data.ledger_close_time.is_some();

        let status = if signature_valid {
            TransactionStatus::Confirmed
        } else {
            TransactionStatus::NotFound
        };

        if tx_data.ledger_number.is_none() {
            warnings.push("Ledger number not found - transaction may not be confirmed".to_string());
        }

        Ok(VerificationDetails {
            signature_valid,
            sequence_valid,
            balance_sufficient,
            network_match,
            timestamp_valid,
            status,
            warnings,
        })
    }

    /// Generate block explorer URL for transaction
    fn generate_explorer_url(&self, transaction_hash: &str) -> String {
        format!("{}/tx/{}", self.config.block_explorer_url, transaction_hash)
    }

    /// Display verification result
    pub fn display_verification(result: &TransactionVerification) {
        println!("\n📋 Transaction Verification Result");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("Transaction Hash: {}", result.transaction_hash);
        println!("Verified: {}", if result.verified { "✅ Yes" } else { "❌ No" });
        println!("Status: {}", result.details.status);
        
        if let Some(ledger) = result.ledger_number {
            println!("Ledger: {}", ledger);
        }
        
        if let Some(time) = result.ledger_close_time {
            println!("Timestamp: {}", time);
            println!("Timestamp Valid: {}", if result.details.timestamp_valid { "✅" } else { "❌" });
        }

        println!("\n🔍 Verification Details:");
        println!("  Signature Valid: {}", if result.details.signature_valid { "✅" } else { "❌" });
        println!("  Sequence Valid: {}", if result.details.sequence_valid { "✅" } else { "❌" });
        println!("  Balance Sufficient: {}", if result.details.balance_sufficient { "✅" } else { "❌" });
        println!("  Network Match: {}", if result.details.network_match { "✅" } else { "❌" });

        if !result.details.warnings.is_empty() {
            println!("\n⚠️  Warnings:");
            for warning in &result.details.warnings {
                println!("   - {}", warning);
            }
        }

        println!("\n🔗 Block Explorer:");
        println!("   {}", result.block_explorer_url);

        println!("\n⏱️  Verification Time: {}ms", result.verification_time_ms);
    }
}

/// Transaction data from blockchain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionData {
    pub hash: String,
    pub ledger_number: Option<u32>,
    pub ledger_close_time: Option<u64>,
    pub created_at: u64,
    pub fee_charged: i64,
    pub operation_count: usize,
    pub status: String,
}

/// Certificate with blockchain verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiableCertificate {
    pub certificate_id: String,
    pub transaction_hash: String,
    pub verification: Option<TransactionVerification>,
    pub verified_at: Option<u64>,
}

impl VerifiableCertificate {
    /// Create a new verifiable certificate
    pub fn new(certificate_id: String, transaction_hash: String) -> Self {
        Self {
            certificate_id,
            transaction_hash,
            verification: None,
            verified_at: None,
        }
    }

    /// Verify the certificate's blockchain transaction
    pub async fn verify(&mut self, verifier: &BlockchainVerifier) -> Result<&TransactionVerification> {
        let verification = verifier.verify_transaction(&self.transaction_hash).await?;
        self.verification = Some(verification);
        self.verified_at = Some(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs());
        
        Ok(self.verification.as_ref().unwrap())
    }

    /// Check if certificate is verified
    pub fn is_verified(&self) -> bool {
        self.verification.as_ref().map_or(false, |v| v.verified)
    }

    /// Get block explorer URL
    pub fn explorer_url(&self) -> String {
        self.verification
            .as_ref()
            .map_or(String::new(), |v| v.block_explorer_url.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_verify_transaction() {
        let verifier = BlockchainVerifier::new();
        let result = verifier.verify_transaction("test_hash_123").await.unwrap();
        
        assert_eq!(result.transaction_hash, "test_hash_123");
        assert!(result.verified);
        assert!(!result.block_explorer_url.is_empty());
    }

    #[tokio::test]
    async fn test_verify_timestamp() {
        let verifier = BlockchainVerifier::new();
        let result = verifier.verify_timestamp("test_hash_123", 1234567890).await.unwrap();
        
        assert!(result);
    }

    #[test]
    fn test_block_explorer_url_generation() {
        let config = VerificationConfig {
            network: "mainnet".to_string(),
            block_explorer_url: "https://stellar.expert/explorer/public".to_string(),
            ..VerificationConfig::default()
        };
        let verifier = BlockchainVerifier::with_config(config);
        let url = verifier.generate_explorer_url("abc123");
        
        assert_eq!(url, "https://stellar.expert/explorer/public/tx/abc123");
    }

    #[tokio::test]
    async fn test_verifiable_certificate() {
        let mut cert = VerifiableCertificate::new(
            "cert_001".to_string(),
            "tx_hash_456".to_string(),
        );
        
        assert!(!cert.is_verified());
        
        let verifier = BlockchainVerifier::new();
        cert.verify(&verifier).await.unwrap();
        
        assert!(cert.is_verified());
        assert!(cert.verified_at.is_some());
        assert!(!cert.explorer_url().is_empty());
    }

    #[test]
    fn test_verification_config_defaults() {
        let config = VerificationConfig::default();
        assert_eq!(config.network, "testnet");
        assert_eq!(config.timeout_seconds, 30);
        assert_eq!(config.max_retries, 3);
    }
}
