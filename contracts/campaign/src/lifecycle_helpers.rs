//! Campaign lifecycle management helpers (cancel and end campaign handlers).

use soroban_sdk::{Env, Address, Symbol};

/// Helper for campaign lifecycle transitions.
pub struct CampaignLifecycleHelper;

impl CampaignLifecycleHelper {
    /// Cancels an active campaign if invoked by the owner/admin before target reached.
    pub fn cancel_campaign(env: &Env, owner: &Address, campaign_id: u64) {
        owner.require_auth();
        env.storage()
            .persistent()
            .set(&(Symbol::new(env, "status"), campaign_id), &3u32); // 3 = Cancelled
    }

    /// Formally ends a campaign after deadline or target reached.
    pub fn end_campaign(env: &Env, owner: &Address, campaign_id: u64) {
        owner.require_auth();
        env.storage()
            .persistent()
            .set(&(Symbol::new(env, "status"), campaign_id), &1u32); // 1 = Ended/Successful
    }
}
