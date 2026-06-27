use dotenvy::dotenv;
use std::env;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Missing required environment variable: {0}")]
    MissingVar(String),
}

/// Application configuration loaded from environment variables.
#[derive(Debug)]
pub struct Config {
    pub stellar_network: String,
    pub stellar_platform_secret: String,
    pub horizon_url: String,
    pub soroban_rpc_url: String,
    pub soroban_network_passphrase: String,
}

impl Config {
    /// Load and validate all required environment variables.
    /// Call this once at startup. Returns an error with a clear message if any var is missing.
    pub fn from_env() -> Result<Self, ConfigError> {
        // Load .env file if present; ignore error if it does not exist.
        let _ = dotenv();

        fn require(key: &str) -> Result<String, ConfigError> {
            env::var(key).map_err(|_| ConfigError::MissingVar(key.to_string()))
        }

        Ok(Self {
            stellar_network: require("STELLAR_NETWORK")?,
            stellar_platform_secret: require("STELLAR_PLATFORM_SECRET")?,
            horizon_url: require("HORIZON_URL")?,
            soroban_rpc_url: require("SOROBAN_RPC_URL")?,
            soroban_network_passphrase: require("SOROBAN_NETWORK_PASSPHRASE")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_var_returns_error() {
        // Unset all required vars to test error path
        for key in &["STELLAR_NETWORK", "STELLAR_PLATFORM_SECRET", "HORIZON_URL", "SOROBAN_RPC_URL", "SOROBAN_NETWORK_PASSPHRASE"] {
            env::remove_var(key);
        }
        let result = Config::from_env();
        assert!(result.is_err());
    }
}