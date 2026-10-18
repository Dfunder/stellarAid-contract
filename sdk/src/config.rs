use dotenvy::dotenv;
use std::env;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Missing required environment variable: {0}")]
    MissingVar(String),
    #[error("Invalid URL for {0}: {1}")]
    InvalidUrl(String, String),
    #[error("Unknown network passphrase: {0}")]
    UnknownNetwork(String),
    #[error(
        "Network passphrase does not match network '{network}': expected '{expected}', got '{actual}'"
    )]
    NetworkMismatch {
        network: String,
        expected: String,
        actual: String,
    },
}

/// Predefined network configurations for Stellar.
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub name: &'static str,
    pub horizon_url: &'static str,
    pub soroban_rpc_url: &'static str,
    pub network_passphrase: &'static str,
}

pub const TESTNET: NetworkConfig = NetworkConfig {
    name: "testnet",
    horizon_url: "https://horizon-testnet.stellar.org",
    soroban_rpc_url: "https://soroban-testnet.stellar.org",
    network_passphrase: "Test SDF Network ; September 2015",
};

pub const MAINNET: NetworkConfig = NetworkConfig {
    name: "mainnet",
    horizon_url: "https://horizon.stellar.org",
    soroban_rpc_url: "https://rpc.mainnet.soroban.stellar.org",
    network_passphrase: "Public Global Stellar Network ; September 2015",
};

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

    /// Create a Config from a predefined NetworkConfig + a secret key.
    /// Useful for scripting against known networks without setting env vars.
    pub fn from_network(network: &NetworkConfig, secret: &str) -> Self {
        Self {
            stellar_network: network.name.to_string(),
            stellar_platform_secret: secret.to_string(),
            horizon_url: network.horizon_url.to_string(),
            soroban_rpc_url: network.soroban_rpc_url.to_string(),
            soroban_network_passphrase: network.network_passphrase.to_string(),
        }
    }

    /// Validate the *contents* of the configuration, not just their presence
    /// (#980).
    ///
    /// Checks that both URLs parse as `http(s)` URLs, and that the network
    /// passphrase matches the named network — or, when `stellar_network` is not
    /// recognised, is at least one of the two known networks. A wrong
    /// passphrase is the single most common integration mistake and produces
    /// the most confusing failure, so it is caught here, immediately, with a
    /// clear message.
    pub fn validate(&self) -> Result<(), ConfigError> {
        Self::validate_url("HORIZON_URL", &self.horizon_url)?;
        Self::validate_url("SOROBAN_RPC_URL", &self.soroban_rpc_url)?;

        match Self::known_passphrase(&self.stellar_network) {
            Some(expected) if self.soroban_network_passphrase != expected => {
                Err(ConfigError::NetworkMismatch {
                    network: self.stellar_network.clone(),
                    expected: expected.to_string(),
                    actual: self.soroban_network_passphrase.clone(),
                })
            }
            Some(_) => Ok(()),
            None => {
                if matches!(
                    self.soroban_network_passphrase.as_str(),
                    TESTNET.network_passphrase | MAINNET.network_passphrase
                ) {
                    Ok(())
                } else {
                    Err(ConfigError::UnknownNetwork(
                        self.soroban_network_passphrase.clone(),
                    ))
                }
            }
        }
    }

    fn validate_url(key: &str, value: &str) -> Result<(), ConfigError> {
        match reqwest::Url::parse(value) {
            Ok(u)
                if matches!(u.scheme(), "http" | "https") && u.host_str().is_some() =>
            {
                Ok(())
            }
            _ => Err(ConfigError::InvalidUrl(key.to_string(), value.to_string())),
        }
    }

    fn known_passphrase(network: &str) -> Option<&'static str> {
        match network.trim().to_ascii_lowercase().as_str() {
            "testnet" => Some(TESTNET.network_passphrase),
            "mainnet" | "public" => Some(MAINNET.network_passphrase),
            _ => None,
        }
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

    #[test]
    fn validate_accepts_a_known_network() {
        let cfg = Config::from_network(&TESTNET, "SDUMMY");
        assert!(cfg.validate().is_ok());
        let cfg = Config::from_network(&MAINNET, "SDUMMY");
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn validate_rejects_a_wrong_passphrase_immediately() {
        // Everything is present and the URL is fine, but the passphrase is for
        // the other network — the most common integration mistake.
        let mut cfg = Config::from_network(&TESTNET, "SDUMMY");
        cfg.soroban_network_passphrase = MAINNET.network_passphrase.to_string();
        let err = cfg.validate().unwrap_err();
        assert!(matches!(err, ConfigError::NetworkMismatch { .. }));
        assert!(err.to_string().contains("does not match"));
    }

    #[test]
    fn validate_rejects_a_malformed_url() {
        let mut cfg = Config::from_network(&TESTNET, "SDUMMY");
        cfg.horizon_url = "not a url".to_string();
        assert!(matches!(
            cfg.validate().unwrap_err(),
            ConfigError::InvalidUrl(_, _)
        ));

        let mut cfg = Config::from_network(&TESTNET, "SDUMMY");
        cfg.soroban_rpc_url = "ftp://rpc.example.com".to_string();
        assert!(matches!(
            cfg.validate().unwrap_err(),
            ConfigError::InvalidUrl(_, _)
        ));
    }

    #[test]
    fn validate_rejects_an_unknown_passphrase_for_an_unknown_network() {
        let mut cfg = Config::from_network(&TESTNET, "SDUMMY");
        cfg.stellar_network = "futurenet".to_string();
        cfg.soroban_network_passphrase = "Future Network ; 2030".to_string();
        assert!(matches!(
            cfg.validate().unwrap_err(),
            ConfigError::UnknownNetwork(_)
        ));
    }
}