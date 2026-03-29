//! Environment-Specific Configuration Manager
//!
//! Provides isolated configurations for different environments:
//! - Testnet: Development and testing
//! - Mainnet: Production
//! - Sandbox: Local development
//!
//! Features:
//! - Environment isolation
//! - Secure credential management
//! - Access control policies
//! - Configuration validation

use crate::secure_vault::{AccessLevel, KeyVault, VaultConfigBuilder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Environment configuration error types
#[derive(Debug, Error)]
pub enum EnvConfigError {
    #[error("Environment not found: {0}")]
    EnvironmentNotFound(String),
    #[error("Missing required configuration: {0}")]
    MissingConfiguration(String),
    #[error("Invalid environment configuration: {0}")]
    InvalidConfiguration(String),
    #[error("Vault error: {0}")]
    Vault(#[from] crate::secure_vault::VaultError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Environment variable not set: {0}")]
    EnvVarNotSet(String),
}

pub type EnvConfigResult<T> = Result<T, EnvConfigError>;

/// Supported environment types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnvironmentType {
    Testnet,
    Mainnet,
    Sandbox,
    Custom(String),
}

impl EnvironmentType {
    pub fn as_str(&self) -> &str {
        match self {
            EnvironmentType::Testnet => "testnet",
            EnvironmentType::Mainnet => "mainnet",
            EnvironmentType::Sandbox => "sandbox",
            EnvironmentType::Custom(s) => s,
        }
    }
}

impl std::fmt::Display for EnvironmentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Environment-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    /// Environment name
    pub name: String,
    /// Environment type
    pub env_type: EnvironmentType,
    /// RPC URL
    pub rpc_url: String,
    /// Horizon URL
    pub horizon_url: String,
    /// Network passphrase
    pub network_passphrase: String,
    /// Whether this is a production environment
    pub is_production: bool,
    /// Access control settings
    pub access_control: AccessControlConfig,
    /// Retry configuration
    pub retry_config: RetryPolicyConfig,
}

/// Access control configuration for environment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessControlConfig {
    /// Require admin authentication for deployments
    pub require_admin_auth: bool,
    /// Require multi-signature for withdrawals
    pub require_multisig: bool,
    /// Maximum transaction amount (in stroops)
    pub max_transaction_amount: Option<i128>,
    /// Allowed operations
    pub allowed_operations: Vec<String>,
    /// IP whitelist (optional)
    pub ip_whitelist: Option<Vec<String>>,
}

/// Retry policy configuration per environment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicyConfig {
    /// Maximum retry attempts
    pub max_attempts: u32,
    /// Initial backoff in milliseconds
    pub initial_backoff_ms: u64,
    /// Maximum backoff in milliseconds
    pub max_backoff_ms: u64,
    /// Enable exponential backoff
    pub exponential_backoff: bool,
    /// Add jitter to backoff
    pub use_jitter: bool,
}

impl Default for RetryPolicyConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff_ms: 100,
            max_backoff_ms: 30000,
            exponential_backoff: true,
            use_jitter: true,
        }
    }
}

/// Environment manager for handling multiple environments
pub struct EnvironmentManager {
    environments: HashMap<String, EnvironmentConfig>,
    current_environment: String,
    vault: Option<KeyVault>,
    config_path: PathBuf,
}

impl EnvironmentManager {
    /// Create a new environment manager
    pub fn new(config_path: &Path) -> EnvConfigResult<Self> {
        let mut manager = Self {
            environments: HashMap::new(),
            current_environment: "testnet".to_string(),
            vault: None,
            config_path: config_path.to_path_buf(),
        };

        // Load default environments
        manager.load_default_environments()?;

        // Load custom environments from file if exists
        if config_path.exists() {
            manager.load_custom_environments()?;
        }

        Ok(manager)
    }

    /// Initialize secure vault
    pub fn initialize_vault(&mut self, master_password: &str) -> EnvConfigResult<()> {
        let vault_path = self.config_path.parent()
            .unwrap_or(Path::new("."))
            .join(format!(".vault_{}.json", self.current_environment));

        let config = VaultConfigBuilder::new()
            .master_password(master_password)
            .vault_path(&vault_path)
            .environment(&self.current_environment)
            .auto_backup(true)
            .build()
            .map_err(|e| EnvConfigError::InvalidConfiguration(e.to_string()))?;

        let vault = KeyVault::new(config)
            .map_err(|e| EnvConfigError::Vault(e))?;

        self.vault = Some(vault);
        Ok(())
    }

    /// Get current environment configuration
    pub fn get_current_config(&self) -> EnvConfigResult<&EnvironmentConfig> {
        self.environments
            .get(&self.current_environment)
            .ok_or_else(|| EnvConfigError::EnvironmentNotFound(self.current_environment.clone()))
    }

    /// Switch to a different environment
    pub fn switch_environment(&mut self, env_name: &str) -> EnvConfigResult<()> {
        if !self.environments.contains_key(env_name) {
            return Err(EnvConfigError::EnvironmentNotFound(env_name.to_string()));
        }

        self.current_environment = env_name.to_string();
        
        // Reinitialize vault for new environment if password was provided
        // (This would be called after initialize_vault with the same password)
        
        Ok(())
    }

    /// List all available environments
    pub fn list_environments(&self) -> Vec<&String> {
        self.environments.keys().collect()
    }

    /// Store a sensitive credential in the vault
    pub fn store_credential(
        &mut self,
        key_id: &str,
        credential: &[u8],
        purpose: &str,
        access_level: AccessLevel,
    ) -> EnvConfigResult<()> {
        let vault = self
            .vault
            .as_mut()
            .ok_or_else(|| EnvConfigError::InvalidConfiguration("Vault not initialized".to_string()))?;

        vault
            .store_key(key_id, credential, purpose, access_level, None)
            .map_err(|e| EnvConfigError::Vault(e))?;

        Ok(())
    }

    /// Retrieve a credential from the vault
    pub fn retrieve_credential(
        &mut self,
        key_id: &str,
        access_level: AccessLevel,
    ) -> EnvConfigResult<Vec<u8>> {
        let vault = self
            .vault
            .as_mut()
            .ok_or_else(|| EnvConfigError::InvalidConfiguration("Vault not initialized".to_string()))?;

        vault
            .retrieve_key(key_id, access_level)
            .map_err(|e| EnvConfigError::Vault(e))
    }

    /// Validate environment configuration
    pub fn validate_environment(&self, env_name: &str) -> EnvConfigResult<Vec<String>> {
        let config = self
            .environments
            .get(env_name)
            .ok_or_else(|| EnvConfigError::EnvironmentNotFound(env_name.to_string()))?;

        let mut warnings = Vec::new();

        // Check for production safety
        if config.is_production {
            if !config.access_control.require_admin_auth {
                warnings.push(
                    "Production environment should require admin authentication".to_string()
                );
            }

            if !config.access_control.require_multisig {
                warnings.push(
                    "Production environment should require multi-signature for withdrawals"
                        .to_string()
                );
            }

            if config.access_control.max_transaction_amount.is_none() {
                warnings.push(
                    "Production environment should have transaction amount limits".to_string()
                );
            }
        }

        // Check retry configuration
        if config.retry_config.max_attempts > 10 {
            warnings.push(format!(
                "High retry count ({}) may cause issues",
                config.retry_config.max_attempts
            ));
        }

        if config.retry_config.initial_backoff_ms < 50 {
            warnings.push(format!(
                "Very short initial backoff ({}ms) may overwhelm API",
                config.retry_config.initial_backoff_ms
            ));
        }

        Ok(warnings)
    }

    /// Get environment-specific retry configuration
    pub fn get_retry_config(&self) -> EnvConfigResult<RetryPolicyConfig> {
        Ok(self.get_current_config()?.retry_config.clone())
    }

    /// Check if current environment is production
    pub fn is_production(&self) -> bool {
        self.environments
            .get(&self.current_environment)
            .map(|c| c.is_production)
            .unwrap_or(false)
    }

    /// Load default environments
    fn load_default_environments(&mut self) -> EnvConfigResult<()> {
        // Testnet
        self.environments.insert(
            "testnet".to_string(),
            EnvironmentConfig {
                name: "testnet".to_string(),
                env_type: EnvironmentType::Testnet,
                rpc_url: env::var("SOROBAN_RPC_URL")
                    .unwrap_or_else(|_| "https://soroban-testnet.stellar.org".to_string()),
                horizon_url: env::var("SOROBAN_HORIZON_URL")
                    .unwrap_or_else(|_| "https://horizon-testnet.stellar.org".to_string()),
                network_passphrase: env::var("SOROBAN_NETWORK_PASSPHRASE")
                    .unwrap_or_else(|_| "Test SDF Network ; September 2015".to_string()),
                is_production: false,
                access_control: AccessControlConfig {
                    require_admin_auth: true,
                    require_multisig: false,
                    max_transaction_amount: None,
                    allowed_operations: vec!["all".to_string()],
                    ip_whitelist: None,
                },
                retry_config: RetryPolicyConfig {
                    max_attempts: 3,
                    initial_backoff_ms: 100,
                    max_backoff_ms: 30000,
                    exponential_backoff: true,
                    use_jitter: true,
                },
            },
        );

        // Mainnet
        self.environments.insert(
            "mainnet".to_string(),
            EnvironmentConfig {
                name: "mainnet".to_string(),
                env_type: EnvironmentType::Mainnet,
                rpc_url: env::var("SOROBAN_RPC_URL")
                    .unwrap_or_else(|_| "https://soroban-mainnet.stellar.org".to_string()),
                horizon_url: env::var("SOROBAN_HORIZON_URL")
                    .unwrap_or_else(|_| "https://horizon.stellar.org".to_string()),
                network_passphrase: env::var("SOROBAN_NETWORK_PASSPHRASE")
                    .unwrap_or_else(|_| "Public Global Stellar Network ; September 2015".to_string()),
                is_production: true,
                access_control: AccessControlConfig {
                    require_admin_auth: true,
                    require_multisig: true,
                    max_transaction_amount: Some(1_000_000_000_000), // 1M XLM
                    allowed_operations: vec!["deploy".to_string(), "invoke".to_string(), "withdraw".to_string()],
                    ip_whitelist: None,
                },
                retry_config: RetryPolicyConfig {
                    max_attempts: 5,
                    initial_backoff_ms: 200,
                    max_backoff_ms: 60000,
                    exponential_backoff: true,
                    use_jitter: true,
                },
            },
        );

        // Sandbox
        self.environments.insert(
            "sandbox".to_string(),
            EnvironmentConfig {
                name: "sandbox".to_string(),
                env_type: EnvironmentType::Sandbox,
                rpc_url: env::var("SOROBAN_RPC_URL")
                    .unwrap_or_else(|_| "http://localhost:8000".to_string()),
                horizon_url: env::var("SOROBAN_HORIZON_URL")
                    .unwrap_or_else(|_| "http://localhost:8000".to_string()),
                network_passphrase: env::var("SOROBAN_NETWORK_PASSPHRASE")
                    .unwrap_or_else(|_| "Standalone Network ; February 2017".to_string()),
                is_production: false,
                access_control: AccessControlConfig {
                    require_admin_auth: false,
                    require_multisig: false,
                    max_transaction_amount: None,
                    allowed_operations: vec!["all".to_string()],
                    ip_whitelist: None,
                },
                retry_config: RetryPolicyConfig {
                    max_attempts: 2,
                    initial_backoff_ms: 50,
                    max_backoff_ms: 5000,
                    exponential_backoff: false,
                    use_jitter: false,
                },
            },
        );

        Ok(())
    }

    /// Load custom environments from configuration file
    fn load_custom_environments(&mut self) -> EnvConfigResult<()> {
        // Implementation for loading from JSON/TOML file
        // This can be extended later
        Ok(())
    }
}

/// Builder for creating environment configurations
pub struct EnvironmentConfigBuilder {
    name: String,
    env_type: EnvironmentType,
    rpc_url: Option<String>,
    horizon_url: Option<String>,
    network_passphrase: Option<String>,
    is_production: bool,
    access_control: AccessControlConfig,
    retry_config: RetryPolicyConfig,
}

impl EnvironmentConfigBuilder {
    pub fn new(name: &str, env_type: EnvironmentType) -> Self {
        Self {
            name: name.to_string(),
            env_type,
            rpc_url: None,
            horizon_url: None,
            network_passphrase: None,
            is_production: false,
            access_control: AccessControlConfig {
                require_admin_auth: true,
                require_multisig: false,
                max_transaction_amount: None,
                allowed_operations: vec!["all".to_string()],
                ip_whitelist: None,
            },
            retry_config: RetryPolicyConfig::default(),
        }
    }

    pub fn rpc_url(mut self, url: &str) -> Self {
        self.rpc_url = Some(url.to_string());
        self
    }

    pub fn horizon_url(mut self, url: &str) -> Self {
        self.horizon_url = Some(url.to_string());
        self
    }

    pub fn network_passphrase(mut self, phrase: &str) -> Self {
        self.network_passphrase = Some(phrase.to_string());
        self
    }

    pub fn production(mut self, is_prod: bool) -> Self {
        self.is_production = is_prod;
        self
    }

    pub fn access_control(mut self, config: AccessControlConfig) -> Self {
        self.access_control = config;
        self
    }

    pub fn retry_config(mut self, config: RetryPolicyConfig) -> Self {
        self.retry_config = config;
        self
    }

    pub fn build(self) -> EnvConfigResult<EnvironmentConfig> {
        Ok(EnvironmentConfig {
            name: self.name,
            env_type: self.env_type,
            rpc_url: self.rpc_url.ok_or_else(|| {
                EnvConfigError::MissingConfiguration("RPC URL".to_string())
            })?,
            horizon_url: self.horizon_url.ok_or_else(|| {
                EnvConfigError::MissingConfiguration("Horizon URL".to_string())
            })?,
            network_passphrase: self.network_passphrase.ok_or_else(|| {
                EnvConfigError::MissingConfiguration("Network passphrase".to_string())
            })?,
            is_production: self.is_production,
            access_control: self.access_control,
            retry_config: self.retry_config,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_environment_manager_creation() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("env_config.json");

        let manager = EnvironmentManager::new(&config_path).unwrap();
        assert_eq!(manager.list_environments().len(), 3);
        assert!(manager.list_environments().contains(&&"testnet".to_string()));
    }

    #[test]
    fn test_environment_switching() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("env_config.json");

        let mut manager = EnvironmentManager::new(&config_path).unwrap();
        
        // Switch to mainnet
        manager.switch_environment("mainnet").unwrap();
        assert_eq!(manager.current_environment, "mainnet");
        
        // Switch to sandbox
        manager.switch_environment("sandbox").unwrap();
        assert_eq!(manager.current_environment, "sandbox");
    }

    #[test]
    fn test_production_detection() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("env_config.json");

        let mut manager = EnvironmentManager::new(&config_path).unwrap();
        
        assert!(!manager.is_production());
        
        manager.switch_environment("mainnet").unwrap();
        assert!(manager.is_production());
    }
}
