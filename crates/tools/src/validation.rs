use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone)]
pub enum ValidationError {
    InvalidAddress(String),
    InvalidAmount(String),
    InvalidAssetCode(String),
    InvalidContractId(String),
    InvalidTransactionHash(String),
    InvalidNetwork(String),
    InvalidPrivateKey(String),
    InvalidMnemonic(String),
    InvalidRange(String),
    MissingRequiredField(String),
    InvalidFormat(String),
    OutOfRange(String),
    NetworkError(String),
    ConfigurationError(String),
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationError::InvalidAddress(addr) => write!(f, "Invalid Stellar address: {}", addr),
            ValidationError::InvalidAmount(amount) => write!(f, "Invalid amount: {}", amount),
            ValidationError::InvalidAssetCode(code) => write!(f, "Invalid asset code: {}", code),
            ValidationError::InvalidContractId(id) => write!(f, "Invalid contract ID: {}", id),
            ValidationError::InvalidTransactionHash(hash) => write!(f, "Invalid transaction hash: {}", hash),
            ValidationError::InvalidNetwork(network) => write!(f, "Invalid network: {}", network),
            ValidationError::InvalidPrivateKey(key) => write!(f, "Invalid private key format"),
            ValidationError::InvalidMnemonic(mnemonic) => write!(f, "Invalid mnemonic phrase"),
            ValidationError::InvalidRange(range) => write!(f, "Invalid range: {}", range),
            ValidationError::MissingRequiredField(field) => write!(f, "Missing required field: {}", field),
            ValidationError::InvalidFormat(format) => write!(f, "Invalid format: {}", format),
            ValidationError::OutOfRange(value) => write!(f, "Value out of range: {}", value),
            ValidationError::NetworkError(err) => write!(f, "Network error: {}", err),
            ValidationError::ConfigurationError(err) => write!(f, "Configuration error: {}", err),
        }
    }
}

impl std::error::Error for ValidationError {}

pub struct InputValidator;

impl InputValidator {
    pub fn validate_stellar_address(address: &str) -> Result<()> {
        if address.is_empty() {
            return Err(ValidationError::InvalidAddress("empty address".to_string()).into());
        }
        
        if !address.starts_with('G') {
            return Err(ValidationError::InvalidAddress(address.to_string()).into());
        }
        
        if address.len() != 56 {
            return Err(ValidationError::InvalidAddress(format!("length {} (expected 56)", address.len())).into());
        }
        
        // Check if all characters are valid base32
        for c in address.chars() {
            if !c.is_ascii_alphanumeric() {
                return Err(ValidationError::InvalidAddress(format!("invalid character '{}'", c)).into());
            }
        }
        
        // Try to parse with stellar-baselib
        stellar_baselib::strkey::StrKey::parse_stellar_account(address)
            .map_err(|e| ValidationError::InvalidAddress(format!("parse error: {}", e)))?;
        
        Ok(())
    }
    
    pub fn validate_amount(amount: &str) -> Result<()> {
        if amount.is_empty() {
            return Err(ValidationError::InvalidAmount("empty amount".to_string()).into());
        }
        
        // Check if amount is a valid number
        let parsed = amount.parse::<f64>()
            .map_err(|_| ValidationError::InvalidAmount(format!("not a number: {}", amount)))?;
        
        if parsed <= 0.0 {
            return Err(ValidationError::InvalidAmount("amount must be positive".to_string()).into());
        }
        
        // Check decimal places (Stellar supports up to 7 decimals)
        if let Some(decimal_pos) = amount.find('.') {
            let decimal_places = amount.len() - decimal_pos - 1;
            if decimal_places > 7 {
                return Err(ValidationError::InvalidAmount(format!("too many decimal places: {} (max 7)", decimal_places)).into());
            }
        }
        
        // Check for reasonable maximum (1 billion XLM)
        if parsed > 1_000_000_000.0 {
            return Err(ValidationError::InvalidAmount("amount exceeds maximum (1B XLM)".to_string()).into());
        }
        
        Ok(())
    }
    
    pub fn validate_asset_code(code: &str) -> Result<()> {
        if code.is_empty() {
            return Err(ValidationError::InvalidAssetCode("empty asset code".to_string()).into());
        }
        
        if code == "XLM" {
            return Ok(()); // Native asset
        }
        
        if code.len() > 12 {
            return Err(ValidationError::InvalidAssetCode(format!("asset code too long: {} (max 12)", code.len())).into());
        }
        
        if code.len() < 1 {
            return Err(ValidationError::InvalidAssetCode("asset code too short".to_string()).into());
        }
        
        // Check if all characters are alphanumeric
        for c in code.chars() {
            if !c.is_ascii_alphanumeric() {
                return Err(ValidationError::InvalidAssetCode(format!("invalid character '{}'", c)).into());
            }
        }
        
        Ok(())
    }
    
    pub fn validate_contract_id(contract_id: &str) -> Result<()> {
        if contract_id.is_empty() {
            return Err(ValidationError::InvalidContractId("empty contract ID".to_string()).into());
        }
        
        if !contract_id.starts_with('C') {
            return Err(ValidationError::InvalidContractId(contract_id.to_string()).into());
        }
        
        if contract_id.len() != 56 {
            return Err(ValidationError::InvalidContractId(format!("length {} (expected 56)", contract_id.len())).into());
        }
        
        // Try to parse as contract strkey
        stellar_baselib::strkey::StrKey::parse_stellar_contract(contract_id)
            .map_err(|e| ValidationError::InvalidContractId(format!("parse error: {}", e)))?;
        
        Ok(())
    }
    
    pub fn validate_transaction_hash(hash: &str) -> Result<()> {
        if hash.is_empty() {
            return Err(ValidationError::InvalidTransactionHash("empty hash".to_string()).into());
        }
        
        if hash.len() != 64 {
            return Err(ValidationError::InvalidTransactionHash(format!("length {} (expected 64)", hash.len())).into());
        }
        
        // Check if all characters are valid hex
        for c in hash.chars() {
            if !c.is_ascii_hexdigit() {
                return Err(ValidationError::InvalidTransactionHash(format!("invalid hex character '{}'", c)).into());
            }
        }
        
        Ok(())
    }
    
    pub fn validate_network(network: &str) -> Result<()> {
        match network {
            "testnet" | "mainnet" | "sandbox" | "public" | "future" => Ok(()),
            _ => Err(ValidationError::InvalidNetwork(format!("'{}' (expected: testnet, mainnet, sandbox, public, future)", network)).into()),
        }
    }
    
    pub fn validate_private_key(private_key: &str) -> Result<()> {
        if private_key.is_empty() {
            return Err(ValidationError::InvalidPrivateKey("empty private key".to_string()).into());
        }
        
        if !private_key.starts_with('S') {
            return Err(ValidationError::InvalidPrivateKey("private key must start with 'S'".to_string()).into());
        }
        
        if private_key.len() != 56 {
            return Err(ValidationError::InvalidPrivateKey(format!("length {} (expected 56)", private_key.len())).into());
        }
        
        // Try to create keypair from secret
        stellar_baselib::keypair::KeyPair::from_secret_key(private_key)
            .map_err(|e| ValidationError::InvalidPrivateKey(format!("invalid key: {}", e)))?;
        
        Ok(())
    }
    
    pub fn validate_mnemonic(mnemonic: &str) -> Result<()> {
        if mnemonic.is_empty() {
            return Err(ValidationError::InvalidMnemonic("empty mnemonic".to_string()).into());
        }
        
        let words: Vec<&str> = mnemonic.split_whitespace().collect();
        
        // Check word count (typically 12, 15, 18, 21, or 24)
        if ![12, 15, 18, 21, 24].contains(&words.len()) {
            return Err(ValidationError::InvalidMnemonic(format!("invalid word count: {} (expected 12, 15, 18, 21, or 24)", words.len())).into());
        }
        
        // Basic validation - in practice you'd use BIP39 validation
        for word in words {
            if word.len() < 3 || word.len() > 8 {
                return Err(ValidationError::InvalidMnemonic(format!("invalid word length: '{}'", word)).into());
            }
        }
        
        Ok(())
    }
    
    pub fn validate_range(value: &str, min: f64, max: f64) -> Result<()> {
        let parsed = value.parse::<f64>()
            .map_err(|_| ValidationError::InvalidRange(format!("not a number: {}", value)))?;
        
        if parsed < min || parsed > max {
            return Err(ValidationError::OutOfRange(format!("{} (must be between {} and {})", parsed, min, max)).into());
        }
        
        Ok(())
    }
    
    pub fn validate_required_fields(
        params: &HashMap<String, String>,
        required_fields: &[&str],
    ) -> Result<()> {
        for field in required_fields {
            if !params.contains_key(*field) || params.get(*field).unwrap().is_empty() {
                return Err(ValidationError::MissingRequiredField(field.to_string()).into());
            }
        }
        
        Ok(())
    }
    
    pub fn validate_url(url: &str) -> Result<()> {
        if url.is_empty() {
            return Err(ValidationError::InvalidFormat("empty URL".to_string()).into());
        }
        
        url::Url::parse(url)
            .map_err(|e| ValidationError::InvalidFormat(format!("invalid URL: {}", e)))?;
        
        Ok(())
    }
    
    pub fn validate_file_path(path: &str) -> Result<()> {
        if path.is_empty() {
            return Err(ValidationError::InvalidFormat("empty file path".to_string()).into());
        }
        
        let path_buf = std::path::Path::new(path);
        
        if path_buf.is_absolute() {
            // Check parent directory exists
            if let Some(parent) = path_buf.parent() {
                if !parent.exists() {
                    return Err(ValidationError::ConfigurationError(format!("parent directory does not exist: {}", parent.display())).into());
                }
            }
        }
        
        Ok(())
    }
    
    pub fn validate_batch_size(size: usize) -> Result<()> {
        if size == 0 {
            return Err(ValidationError::InvalidRange("batch size cannot be zero".to_string()).into());
        }
        
        if size > 1000 {
            return Err(ValidationError::OutOfRange(format!("batch size {} (max 1000)", size)).into());
        }
        
        Ok(())
    }
    
    pub fn validate_timeout(timeout: u64) -> Result<()> {
        if timeout == 0 {
            return Err(ValidationError::InvalidRange("timeout cannot be zero".to_string()).into());
        }
        
        if timeout > 3600 {
            return Err(ValidationError::OutOfRange(format!("timeout {}s (max 3600s)", timeout)).into());
        }
        
        Ok(())
    }
}

pub struct ErrorHandler;

impl ErrorHandler {
    pub fn handle_horizon_error(error: &crate::horizon_error::HorizonError) -> Result<()> {
        match error.status_code {
            400 => Err(anyhow!("Bad Request: {}", error.message)),
            401 => Err(anyhow!("Unauthorized: {}", error.message)),
            403 => Err(anyhow!("Forbidden: {}", error.message)),
            404 => Err(anyhow!("Not Found: {}", error.message)),
            429 => Err(anyhow!("Rate Limited: {}", error.message)),
            500 => Err(anyhow!("Internal Server Error: {}", error.message)),
            502 => Err(anyhow!("Bad Gateway: {}", error.message)),
            503 => Err(anyhow!("Service Unavailable: {}", error.message)),
            _ => Err(anyhow!("Horizon Error ({}): {}", error.status_code, error.message)),
        }
    }
    
    pub fn handle_transaction_error(result: &serde_json::Value) -> Result<()> {
        if let Some(success) = result.get("successful").and_then(|v| v.as_bool()) {
            if !success {
                let error_result = result.get("result")
                    .and_then(|r| r.get("transaction"))
                    .and_then(|t| t.get("result"))
                    .and_then(|r| r.get("error"));
                
                if let Some(error) = error_result {
                    return Err(anyhow!("Transaction failed: {}", error));
                } else {
                    return Err(anyhow!("Transaction failed: Unknown error"));
                }
            }
        }
        
        Ok(())
    }
    
    pub fn handle_network_error(error: reqwest::Error) -> Result<()> {
        if error.is_timeout() {
            Err(anyhow!("Network timeout: {}", error))
        } else if error.is_connect() {
            Err(anyhow!("Connection failed: {}", error))
        } else if error.is_request() {
            Err(anyhow!("Request error: {}", error))
        } else {
            Err(anyhow!("Network error: {}", error))
        }
    }
    
    pub fn handle_config_error(error: &str) -> Result<()> {
        Err(ValidationError::ConfigurationError(error.to_string()).into())
    }
    
    pub fn format_validation_error(error: &ValidationError) -> String {
        match error {
            ValidationError::InvalidAddress(addr) => {
                format!("❌ Invalid address: {}\n💡 Stellar addresses start with 'G' and are 56 characters long", addr)
            },
            ValidationError::InvalidAmount(amount) => {
                format!("❌ Invalid amount: {}\n💡 Amounts must be positive numbers with up to 7 decimal places", amount)
            },
            ValidationError::InvalidAssetCode(code) => {
                format!("❌ Invalid asset code: {}\n💡 Asset codes must be 1-12 alphanumeric characters (or 'XLM' for native)", code)
            },
            ValidationError::InvalidContractId(id) => {
                format!("❌ Invalid contract ID: {}\n💡 Contract IDs start with 'C' and are 56 characters long", id)
            },
            ValidationError::InvalidTransactionHash(hash) => {
                format!("❌ Invalid transaction hash: {}\n💡 Transaction hashes are 64-character hexadecimal strings", hash)
            },
            ValidationError::InvalidNetwork(network) => {
                format!("❌ Invalid network: {}\n💡 Valid networks are: testnet, mainnet, sandbox, public, future", network)
            },
            ValidationError::InvalidPrivateKey(_) => {
                "❌ Invalid private key\n💡 Private keys start with 'S' and are 56 characters long".to_string()
            },
            ValidationError::InvalidMnemonic(_) => {
                "❌ Invalid mnemonic phrase\n💡 Mnemonic phrases should be 12, 15, 18, 21, or 24 words".to_string()
            },
            ValidationError::MissingRequiredField(field) => {
                format!("❌ Missing required field: {}\n💡 Please provide the required parameter", field)
            },
            ValidationError::InvalidFormat(format_desc) => {
                format!("❌ Invalid format: {}\n💡 Please check the input format", format_desc)
            },
            ValidationError::OutOfRange(value) => {
                format!("❌ Value out of range: {}\n💡 Please check the allowed range", value)
            },
            ValidationError::NetworkError(err) => {
                format!("❌ Network error: {}\n💡 Please check your internet connection and try again", err)
            },
            ValidationError::ConfigurationError(err) => {
                format!("❌ Configuration error: {}\n💡 Please check your configuration settings", err)
            },
            _ => format!("❌ Validation error: {}", error),
        }
    }
    
    pub fn suggest_fix(error: &ValidationError) -> String {
        match error {
            ValidationError::InvalidAddress(_) => {
                "Example: GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K".to_string()
            },
            ValidationError::InvalidAmount(_) => {
                "Example: 10.5 (for 10.5 XLM)".to_string()
            },
            ValidationError::InvalidAssetCode(_) => {
                "Examples: XLM (native), USDC, EURT".to_string()
            },
            ValidationError::InvalidContractId(_) => {
                "Example: CA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA3".to_string()
            },
            ValidationError::InvalidTransactionHash(_) => {
                "Example: a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456".to_string()
            },
            ValidationError::InvalidNetwork(_) => {
                "Use: testnet, mainnet, sandbox, public, or future".to_string()
            },
            ValidationError::InvalidPrivateKey(_) => {
                "Example: SABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K".to_string()
            },
            ValidationError::InvalidMnemonic(_) => {
                "Example: abandon ability able about above absent absorb abstract absurd abuse access accident account accuse achieve acid acoustic acquire across act".to_string()
            },
            ValidationError::MissingRequiredField(field) => {
                format!("Add the missing parameter: --{}", field.replace("_", "-"))
            },
            _ => "Check the command help for correct usage".to_string(),
        }
    }
}
