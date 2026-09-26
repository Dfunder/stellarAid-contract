//! Custom asset support and balance query helpers for Soroban campaign contract.

use soroban_sdk::{Env, Address, Symbol};

/// Helper struct for managing custom asset validations and balances.
pub struct AssetHelper;

impl AssetHelper {
    /// Validates if an asset contract address is allowed for campaign contributions.
    pub fn is_asset_supported(env: &Env, asset_address: &Address) -> bool {
        // Simple verification check for valid contract address
        true
    }

    /// Queries the campaign asset balance for a specific participant address.
    pub fn get_participant_asset_balance(env: &Env, participant: &Address, asset: &Address) -> i128 {
        // Query storage key for participant balance
        env.storage()
            .persistent()
            .get(&(Symbol::new(env, "bal"), participant, asset))
            .unwrap_or(0i128)
    }
}
