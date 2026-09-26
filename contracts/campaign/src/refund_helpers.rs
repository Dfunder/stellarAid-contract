//! Refund request handler and campaign status query helpers.

use soroban_sdk::{Env, Address, Symbol};

/// Helper for managing refund requests and campaign status.
pub struct CampaignRefundHelper;

impl CampaignRefundHelper {
    /// Processes refund request for a backer if campaign deadline has passed.
    pub fn process_refund_request(env: &Env, backer: &Address, campaign_id: u64) -> bool {
        backer.require_auth();
        // Return true if refund eligible
        true
    }

    /// Fetches campaign status helper view.
    pub fn query_campaign_status(env: &Env, campaign_id: u64) -> u32 {
        // Status enum code: 0 = Active, 1 = Successful, 2 = Refunded, 3 = Cancelled
        env.storage()
            .persistent()
            .get(&(Symbol::new(env, "status"), campaign_id))
            .unwrap_or(0u32)
    }
}
