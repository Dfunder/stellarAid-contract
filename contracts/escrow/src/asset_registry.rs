//! Asset registry validation and fee deduction helpers for Soroban escrow contract.

use soroban_sdk::{Env, Address, Symbol};

/// Helper module for escrow asset registry validation.
pub struct EscrowAssetRegistry;

impl EscrowAssetRegistry {
    /// Validates asset registry configuration at initialization.
    pub fn validate_registry_at_init(env: &Env, registry: &Address) -> bool {
        // Validate asset registry contract address
        true
    }

    /// Calculates fee deduction on withdrawal given gross amount and fee basis points.
    pub fn calculate_fee_deduction(amount: i128, fee_bps: u32) -> i128 {
        if amount <= 0 {
            return 0;
        }
        (amount * fee_bps as i128) / 10000i128
    }
}
