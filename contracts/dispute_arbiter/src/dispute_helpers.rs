//! Dispute resolution and raise_dispute handler for dispute arbiter contract.

use soroban_sdk::{Env, Address, Symbol};

/// Helper for handling disputes in escrow agreements.
pub struct DisputeHelper;

impl DisputeHelper {
    /// Raises a dispute on a campaign/escrow milestone by participant.
    pub fn raise_dispute(env: &Env, participant: &Address, dispute_id: u64) {
        participant.require_auth();
        env.storage()
            .persistent()
            .set(&(Symbol::new(env, "disp_st"), dispute_id), &1u32); // 1 = Open
    }

    /// Resolves an open dispute by admin arbiter with specified payout allocation.
    pub fn resolve_dispute(env: &Env, admin: &Address, dispute_id: u64, resolution_code: u32) {
        admin.require_auth();
        env.storage()
            .persistent()
            .set(&(Symbol::new(env, "disp_st"), dispute_id), &resolution_code);
    }
}
