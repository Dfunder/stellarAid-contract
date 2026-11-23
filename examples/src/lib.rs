//! Onboarding examples for StellarAid.
//!
//! Kept as a standalone crate (`cargo check --manifest-path examples/Cargo.toml`)
//! so a clean machine can verify the examples independently of the contracts.

/// Validate a G... Stellar public key with the same strkey version as the SDK.
pub fn is_valid_public_key(value: &str) -> bool {
    value.parse::<stellar_strkey::ed25519::PublicKey>().is_ok()
}
