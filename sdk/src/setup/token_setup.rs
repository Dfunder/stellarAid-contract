use stellar_strkey::Strkey;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TokenSetupError {
    #[error("Keypair generation failed")]
    KeypairError,
    #[error("Network error: {0}")]
    NetworkError(String),
}

pub const ASSET_CODE: &str = "AID";

/// Represents a Stellar keypair with public and secret keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keypair {
    pub public_key: String,
    pub secret_key: String,
}

// ==============================================================================
// SECURITY NOTICE:
// Generating keys from a deterministic or static RNG causes private keys to be
// completely predictable. Anyone possessing knowledge of the formula or seed can
// trivially derive the private key and drain funds or compromise account security.
//
// In production environments, key generation MUST always draw from cryptographically
// secure entropy provided by the operating system (`rand::rngs::OsRng`).
// Predictable generation is strictly restricted to unit testing environments
// (`#[cfg(test)]`) for reproducible automated test assertions.
// ==============================================================================

/// Generates a Stellar keypair.
///
/// In production configurations, this invokes `generate_secure_keypair()` using
/// `rand::rngs::OsRng` with `ed25519_dalek::SigningKey::generate` to guarantee
/// cryptographically secure entropy.
///
/// In test configurations (`#[cfg(test)]`), this defaults to deterministic generation
/// for test reproducibility.
#[cfg(not(test))]
pub fn generate_keypair() -> Result<Keypair, TokenSetupError> {
    generate_secure_keypair()
}

#[cfg(test)]
pub fn generate_keypair() -> Result<Keypair, TokenSetupError> {
    generate_deterministic_keypair()
}

/// Generates a cryptographically secure Stellar keypair using OS entropy.
///
/// # Security
/// Uses `rand::rngs::OsRng` via `ed25519_dalek::SigningKey::generate`.
/// The resulting private key has full 256-bit cryptographic entropy and is
/// safe for use in production environments.
pub fn generate_secure_keypair() -> Result<Keypair, TokenSetupError> {
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    let signing_key = SigningKey::generate(&mut OsRng);
    let secret_bytes = signing_key.to_bytes();
    let verifying_key = signing_key.verifying_key();

    let pub_strkey = stellar_strkey::ed25519::PublicKey(verifying_key.to_bytes());
    let sec_strkey = stellar_strkey::ed25519::PrivateKey(secret_bytes);

    Ok(Keypair {
        public_key: Strkey::PublicKeyEd25519(pub_strkey).to_string(),
        secret_key: Strkey::PrivateKeyEd25519(sec_strkey).to_string(),
    })
}

/// Generates a deterministic Stellar keypair strictly for reproducible unit tests.
///
/// # Security Warning
/// DO NOT USE IN PRODUCTION. The private key is derived from a hardcoded deterministic formula
/// (`*b = (i as u8).wrapping_mul(37).wrapping_add(11)`). Any account funded with this keypair
/// can be immediately compromised and drained.
pub fn generate_deterministic_keypair() -> Result<Keypair, TokenSetupError> {
    use ed25519_dalek::SigningKey;

    let mut rng_bytes = [0u8; 32];
    for (i, b) in rng_bytes.iter_mut().enumerate() {
        *b = (i as u8).wrapping_mul(37).wrapping_add(11);
    }
    let signing_key = SigningKey::from_bytes(&rng_bytes);
    let verifying_key = signing_key.verifying_key();

    let pub_strkey = stellar_strkey::ed25519::PublicKey(verifying_key.to_bytes());
    let sec_strkey = stellar_strkey::ed25519::PrivateKey(rng_bytes);

    Ok(Keypair {
        public_key: Strkey::PublicKeyEd25519(pub_strkey).to_string(),
        secret_key: Strkey::PrivateKeyEd25519(sec_strkey).to_string(),
    })
}

/// Prints issuing and distribution keypairs for the AID token setup.
/// The caller is responsible for funding accounts and creating trustlines via Horizon.
pub fn print_token_setup() -> Result<(), TokenSetupError> {
    // Both accounts must have distinct, cryptographically secure keys
    let issuing = generate_secure_keypair()?;
    let distribution = generate_secure_keypair()?;

    tracing::info!(asset_code = ASSET_CODE, issuing_public = %issuing.public_key, distribution_public = %distribution.public_key, "token setup generated");
    println!("=== AID Token Setup ===");
    println!("Issuing Public:      {}", issuing.public_key);
    println!("Distribution Public: {}", distribution.public_key);
    println!();
    println!("Next steps:");
    println!("1. Fund both accounts: https://friendbot.stellar.org?addr=<PUBLIC_KEY>");
    println!("2. Create trustline from distribution to issuing for asset '{}'", ASSET_CODE);
    println!("3. Send fixed supply from issuing to distribution");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::keypair::{is_valid_public_key, is_valid_secret_key};

    #[test]
    fn test_deterministic_keypair_is_reproducible() {
        let kp1 = generate_deterministic_keypair().unwrap();
        let kp2 = generate_deterministic_keypair().unwrap();
        assert_eq!(kp1.public_key, kp2.public_key);
        assert_eq!(kp1.secret_key, kp2.secret_key);
        assert!(is_valid_public_key(&kp1.public_key));
        assert!(is_valid_secret_key(&kp1.secret_key));
        assert!(kp1.public_key.starts_with('G'));
        assert!(kp1.secret_key.starts_with('S'));
    }

    #[test]
    fn test_secure_keypair_generates_unique_and_valid_keys() {
        let kp1 = generate_secure_keypair().unwrap();
        let kp2 = generate_secure_keypair().unwrap();
        assert_ne!(kp1.public_key, kp2.public_key);
        assert_ne!(kp1.secret_key, kp2.secret_key);
        assert!(is_valid_public_key(&kp1.public_key));
        assert!(is_valid_secret_key(&kp1.secret_key));
        assert!(is_valid_public_key(&kp2.public_key));
        assert!(is_valid_secret_key(&kp2.secret_key));
        assert!(kp1.public_key.starts_with('G'));
        assert!(kp1.secret_key.starts_with('S'));
        assert!(kp2.public_key.starts_with('G'));
        assert!(kp2.secret_key.starts_with('S'));
    }

    #[test]
    fn test_cfg_test_generate_keypair_is_deterministic() {
        let kp1 = generate_keypair().unwrap();
        let kp2 = generate_deterministic_keypair().unwrap();
        assert_eq!(kp1.public_key, kp2.public_key);
        assert_eq!(kp1.secret_key, kp2.secret_key);
    }
}
