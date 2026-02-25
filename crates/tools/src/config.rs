use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Represents a supported Soroban network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Network {
    Testnet,
    Mainnet,
    Sandbox,
    Custom(String),
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Network::Testnet => write!(f, "testnet"),
            Network::Mainnet => write!(f, "mainnet"),
            Network::Sandbox => write!(f, "sandbox"),
            Network::Custom(s) => write!(f, "{}", s),
        }
    }
}

#[derive(Deserialize, Debug)]
struct ProfileFile {
    network: String,
    rpc_url: String,
    network_passphrase: String,
}

#[derive(Deserialize, Debug)]
struct SorobanToml {
    #[serde(rename = "profile")]
    profiles: HashMap<String, ProfileFile>,
}

/// Strongly-typed configuration resolved for runtime use.
#[derive(Debug, Clone)]
pub struct Config {
    /// Logical profile name chosen from `soroban.toml`.
    #[allow(dead_code)]
    pub profile: String,
    /// Resolved network enum.
    pub network: Network,
    /// Resolved RPC URL.
    pub rpc_url: String,
    /// Resolved network passphrase.
    pub network_passphrase: String,
    /// Admin key for contract deployment (from environment or generated).
    pub admin_key: Option<String>,
}

/// Errors that can occur when loading configuration.
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("I/O error reading soroban.toml: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse soroban.toml: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("No profile selected and no default available. Set the SOROBAN_NETWORK env or add a 'testnet' profile to soroban.toml")]
    NoProfileSelected,
    #[error("Profile '{0}' not found in soroban.toml")]
    ProfileNotFound(String),
    #[error("Missing required value: {0}")]
    MissingValue(&'static str),
}

impl Config {
    /// Load configuration using resolution order:
    /// 1. Environment variables (full or partial overrides)
    /// 2. `soroban.toml` selected profile (defaults to `testnet` if present)
    /// 3. Fail with a clear error
    pub fn load(soroban_toml_path: Option<&Path>) -> Result<Self, ConfigError> {
        // Find soroban.toml first (so we can load a nearby `.env` instead of a
        // repository-global one which could make tests non-deterministic).
        let toml_path = match soroban_toml_path {
            Some(p) => p.to_path_buf(),
            None => find_soroban_toml()?,
        };

        // If there's a `.env` next to the provided `soroban.toml`, load it.
        if let Some(parent) = toml_path.parent() {
            let env_path = parent.join(".env");
            if env_path.exists() {
                let _ = dotenvy::from_path(env_path).ok();
            }
        }

        // Read env overrides (may be unset)
        let env_network = env::var("SOROBAN_NETWORK").ok();
        let env_rpc = env::var("SOROBAN_RPC_URL").ok();
        let env_pass = env::var("SOROBAN_NETWORK_PASSPHRASE").ok();
        let env_admin_key = env::var("SOROBAN_ADMIN_KEY").ok();

        let toml_contents = fs::read_to_string(&toml_path)?;
        let toml: SorobanToml = toml::from_str(&toml_contents)?;

        // Determine profile name
        let profile_name = if let Some(ref name) = env_network {
            name.clone()
        } else if toml.profiles.contains_key("testnet") {
            "testnet".to_string()
        } else if toml.profiles.len() == 1 {
            toml.profiles.keys().next().unwrap().clone()
        } else {
            return Err(ConfigError::NoProfileSelected);
        };

        let profile = toml
            .profiles
            .get(&profile_name)
            .ok_or_else(|| ConfigError::ProfileNotFound(profile_name.clone()))?;

        // base values from profile
        let mut rpc_url = profile.rpc_url.clone();
        let mut network_passphrase = profile.network_passphrase.clone();
        let network_str = profile.network.clone();

        // apply env overrides when present
        if let Some(e) = env_rpc {
            rpc_url = e;
        }
        if let Some(e) = env_pass {
            network_passphrase = e;
        }

        // derive Network enum (prefer env_network string if provided)
        let network_enum = match env_network.as_deref().unwrap_or(&network_str) {
            "testnet" => Network::Testnet,
            "mainnet" => Network::Mainnet,
            "sandbox" => Network::Sandbox,
            other => Network::Custom(other.to_string()),
        };

        // ensure required values present
        if rpc_url.trim().is_empty() {
            return Err(ConfigError::MissingValue("SOROBAN_RPC_URL"));
        }
        if network_passphrase.trim().is_empty() {
            return Err(ConfigError::MissingValue("SOROBAN_NETWORK_PASSPHRASE"));
        }

        Ok(Config {
            profile: profile_name,
            network: network_enum,
            rpc_url,
            network_passphrase,
            admin_key: env_admin_key,
        })
    }
}

fn find_soroban_toml() -> Result<PathBuf, ConfigError> {
    let mut dir = env::current_dir()?;
    for _ in 0..10 {
        let candidate = dir.join("soroban.toml");
        if candidate.exists() {
            return Ok(candidate);
        }
        if !dir.pop() {
            break;
        }
    }
    Err(ConfigError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "soroban.toml not found in current or parent directories",
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    fn write_toml(dir: &Path, content: &str) -> PathBuf {
        let p = dir.join("soroban.toml");
        let mut f = File::create(&p).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        p
    }

    fn clear_env_vars() {
        env::remove_var("SOROBAN_NETWORK");
        env::remove_var("SOROBAN_RPC_URL");
        env::remove_var("SOROBAN_NETWORK_PASSPHRASE");
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn with_isolated_env<F: FnOnce()>(f: F) {
        let _guard = env_lock().lock().expect("env lock poisoned");
        clear_env_vars();
        f();
        clear_env_vars();
    }

    #[test]
    fn loads_profile_by_name() {
        with_isolated_env(|| {
            let d = tempdir().unwrap();
            let toml = r#"
[profile.testnet]
network = "testnet"
rpc_url = "https://soroban-testnet.stellar.org"
network_passphrase = "Test SDF Network ; September 2015"
"#;
            let p = write_toml(d.path(), toml);
            env::set_var("SOROBAN_NETWORK", "testnet");

            let cfg = Config::load(Some(&p)).expect("should load");
            assert_eq!(cfg.profile, "testnet");
            assert_eq!(cfg.rpc_url, "https://soroban-testnet.stellar.org");
            assert_eq!(cfg.network_passphrase, "Test SDF Network ; September 2015");
            match cfg.network {
                Network::Testnet => {},
                _ => panic!("expected testnet"),
            }
        });
    }

    #[test]
    fn env_overrides_profile_values() {
        with_isolated_env(|| {
            let d = tempdir().unwrap();
            let toml = r#"
[profile.testnet]
network = "testnet"
rpc_url = "https://soroban-testnet.stellar.org"
network_passphrase = "Test SDF Network ; September 2015"
"#;
            let p = write_toml(d.path(), toml);
            env::set_var("SOROBAN_NETWORK", "testnet");
            env::set_var("SOROBAN_RPC_URL", "https://override.local");
            env::set_var("SOROBAN_NETWORK_PASSPHRASE", "override pass");

            let cfg = Config::load(Some(&p)).expect("should load with overrides");
            assert_eq!(cfg.rpc_url, "https://override.local");
            assert_eq!(cfg.network_passphrase, "override pass");
        });
    }

    #[test]
    fn missing_required_values_returns_error() {
        with_isolated_env(|| {
            let d = tempdir().unwrap();
            // create a profile with empty values
            let toml = r#"
[profile.empty]
network = ""
rpc_url = ""
network_passphrase = ""
"#;
            let p = write_toml(d.path(), toml);

            // ensure defaulting behavior picks testnet is not present -> should error
            let res = Config::load(Some(&p));
            assert!(res.is_err());
        });
    }

    #[test]
    fn loads_sandbox_profile() {
        with_isolated_env(|| {
            let d = tempdir().unwrap();
            let toml = r#"
[profile.sandbox]
network = "sandbox"
rpc_url = "http://localhost:8000"
network_passphrase = "Standalone Network ; February 2017"
"#;
            let p = write_toml(d.path(), toml);

            env::set_var("SOROBAN_NETWORK", "sandbox");

            let cfg = Config::load(Some(&p)).expect("should load sandbox");
            assert_eq!(cfg.profile, "sandbox");
            assert_eq!(cfg.rpc_url, "http://localhost:8000");
        });
    }
}
