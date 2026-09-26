//! Platform fee configuration and refund amount calculation helpers.

use soroban_sdk::{Env, Address, Symbol};

/// Helper for platform fee configuration and refund calculations.
pub struct FeeConfigHelper;

impl FeeConfigHelper {
    /// Configures platform fee percentage (in basis points).
    pub fn set_platform_fee_bps(env: &Env, admin: &Address, fee_bps: u32) {
        admin.require_auth();
        env.storage().instance().set(&Symbol::new(env, "fee_bps"), &fee_bps);
    }

    /// Calculates refundable amount per asset considering platform fees paid.
    pub fn calculate_refund_amount(gross_donation: i128, platform_fee_bps: u32) -> i128 {
        let fee = (gross_donation * platform_fee_bps as i128) / 10000i128;
        gross_donation - fee
    }
}
