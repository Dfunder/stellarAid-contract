#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, String, Symbol};
use shared::pause;
use shared::types::{Campaign, CampaignStatus};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin = 0,
    Initialized = 1,
    Campaign(u64) = 2,
    CampaignCount = 3,
}

#[contracttype]
#[derive(Clone)]
pub struct CampaignRegisteredEvent {
    pub campaign_id: u64,
    pub owner: Address,
    pub goal: i128,
    pub deadline: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct CampaignStatusChangedEvent {
    pub campaign_id: u64,
    pub old_status: CampaignStatus,
    pub new_status: CampaignStatus,
}

const MIN_TTL: u32 = 17280; // 1 day in ledgers (assuming 5s ledger time)
const MAX_TTL: u32 = 6312000; // 1 year in ledgers (assuming 5s ledger time)

/// Maximum number of seconds into the future a deadline may be set.
/// 2 years = 2 * 365.25 * 24 * 3600 ≈ 63_115_200 seconds (closes #592).
const MAX_DEADLINE_OFFSET_SECS: u64 = 63_115_200;

/// Maximum byte length for a campaign-related string input (closes #591).
const MAX_STRING_INPUT_LEN: u32 = 512;

#[contract]
pub struct CampaignContract;

#[contractimpl]
impl CampaignContract {
    /// Initialize the campaign contract with an admin address.
    /// Must be called once before any other operations.
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        if env.storage().instance().has(&DataKey::Initialized) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage().instance().set(&DataKey::CampaignCount, &0_u64);
    }

    /// Pause the contract, blocking all state-changing operations.
    pub fn pause(env: Env, admin: Address) {
        admin.require_auth();
        Self::ensure_admin(&env, &admin);
        pause::pause(&env, &admin);
    }

    /// Unpause the contract, restoring normal operations.
    pub fn unpause(env: Env, admin: Address) {
        admin.require_auth();
        Self::ensure_admin(&env, &admin);
        pause::unpause(&env, &admin);
    }

    /// Create a new fundraising campaign.
    /// Returns the newly assigned campaign ID.
    /// Closes #591 – no string input in this function, validated at caller.
    /// Closes #592 – validates deadline does not exceed 2 years from now.
    pub fn create_campaign(
        env: Env,
        owner: Address,
        goal: i128,
        deadline: u64,
        fee_bps: u32,
        platform_wallet: Option<Address>,
    ) -> u64 {
        pause::require_not_paused(&env);
        owner.require_auth();
        if fee_bps > 1000 {
            panic!("fee_bps must not exceed 1000");
        }
        // ── Deadline upper bound (closes #592) ─────────────────────────────
        let now = env.ledger().timestamp();
        let max_deadline = now.checked_add(MAX_DEADLINE_OFFSET_SECS)
            .expect("deadline arithmetic overflow");
        if deadline > max_deadline {
            panic!("deadline exceeds maximum allowed (2 years from now)");
        }
        if deadline <= now {
            panic!("deadline must be in the future");
        }
        let id = Self::next_campaign_id(&env);
        let campaign = Campaign {
            id,
            owner: owner.clone(),
            goal,
            raised: 0,
            status: CampaignStatus::Active,
            deadline,
            fee_bps,
            platform_wallet,
        };
        env.storage().persistent().set(&DataKey::Campaign(id), &campaign);
        Self::bump_campaign_ttl(env.clone(), id);
        env.events().publish((Symbol::new(&env, "campaign_registered"),), CampaignRegisteredEvent {
            campaign_id: id,
            owner,
            goal,
            deadline,
        });
        id
    }

    /// Get campaign details by ID.
    pub fn get_campaign(env: Env, campaign_id: u64) -> Option<Campaign> {
        env.storage().persistent().get(&DataKey::Campaign(campaign_id))
    }

    /// Update the status of a campaign. Emits a `campaign_status_changed` event
    /// with both old and new status values.
    pub fn update_campaign_status(env: Env, admin: Address, campaign_id: u64, new_status: CampaignStatus) {
        admin.require_auth();
        Self::ensure_admin(&env, &admin);
        let mut campaign = Self::get_campaign(env.clone(), campaign_id).unwrap();
        let old_status = campaign.status.clone();
        campaign.status = new_status.clone();
        env.storage().persistent().set(&DataKey::Campaign(campaign_id), &campaign);
        env.events().publish((Symbol::new(&env, "campaign_status_changed"),), CampaignStatusChangedEvent {
            campaign_id,
            old_status,
            new_status,
        });
    }

    /// Increment the raised amount for a campaign. Called via cross-contract
    /// call from the Donation contract after a successful donation.
    pub fn update_raised(env: Env, campaign_id: u64, amount: i128) {
        pause::require_not_paused(&env);
        let mut campaign = env
            .storage()
            .persistent()
            .get::<DataKey, Campaign>(&DataKey::Campaign(campaign_id))
            .unwrap();
        campaign.raised += amount;
        env.storage().persistent().set(&DataKey::Campaign(campaign_id), &campaign);
        Self::bump_campaign_ttl(env.clone(), campaign_id);
    }

    /// Approve a campaign, moving it to Active status.
    pub fn approve_campaign(env: Env, admin: Address, campaign_id: u64) {
        Self::update_campaign_status(env, admin, campaign_id, CampaignStatus::Active);
    }

    /// Reject a campaign, moving it to Rejected status.
    /// Closes #591 – validates reason length.
    pub fn reject_campaign(env: Env, admin: Address, campaign_id: u64, reason: String) {
        pause::require_not_paused(&env);
        admin.require_auth();
        Self::ensure_admin(&env, &admin);
        // ── Input length validation (closes #591) ──────────────────────────
        if reason.len() > MAX_STRING_INPUT_LEN {
            panic!("reason exceeds maximum allowed length");
        }
        let mut campaign = Self::get_campaign(env.clone(), campaign_id).unwrap();
        let old_status = campaign.status.clone();
        campaign.status = CampaignStatus::Rejected;
        env.storage().persistent().set(&DataKey::Campaign(campaign_id), &campaign);
        env.events().publish((Symbol::new(&env, "campaign_status_changed"),), CampaignStatusChangedEvent {
            campaign_id,
            old_status,
            new_status: CampaignStatus::Rejected,
        });
        let _ = reason;
    }

    /// Suspend a campaign, moving it to Suspended status.
    pub fn suspend_campaign(env: Env, admin: Address, campaign_id: u64) {
        Self::update_campaign_status(env, admin, campaign_id, CampaignStatus::Suspended);
    }

    /// Get the total number of campaigns created.
    pub fn get_campaign_count(env: Env) -> u64 {
        env.storage().instance().get(&DataKey::CampaignCount).unwrap_or(0_u64)
    }

    /// Transfer admin privileges to a new address.
    pub fn transfer_admin(env: Env, current_admin: Address, new_admin: Address) {
        pause::require_not_paused(&env);
        current_admin.require_auth();
        Self::ensure_admin(&env, &current_admin);
        env.storage().instance().set(&DataKey::Admin, &new_admin);
    }

    /// Upgrade the contract to a new WASM implementation.
    pub fn upgrade(env: Env, admin: Address, new_wasm_hash: BytesN<32>) {
        admin.require_auth();
        Self::ensure_admin(&env, &admin);
        env.deployer().update_current_contract_wasm(&new_wasm_hash);
    }

    /// Bumps the TTL of a campaign to ensure it doesn't expire.
    /// Archive (delete) a campaign record.
    /// Only the admin can archive a campaign, and only if its status is
    /// Completed, Rejected, Cancelled, or Suspended (i.e., no active funds
    /// in flight).
    pub fn archive_campaign(env: Env, admin: Address, campaign_id: u64) {
        admin.require_auth();
        Self::ensure_admin(&env, &admin);
        let campaign = Self::get_campaign(env.clone(), campaign_id).unwrap();
        match campaign.status {
            CampaignStatus::Active | CampaignStatus::Pending => {
                panic!("cannot archive an active or pending campaign");
            }
            _ => {}
        }
        env.storage().persistent().remove(&DataKey::Campaign(campaign_id));
        env.events().publish(
            (Symbol::new(&env, "campaign_archived"),),
            (campaign_id,),
        );
    }

    pub fn get_fee_config(env: Env, campaign_id: u64) -> (u32, Option<Address>) {
        let campaign = Self::get_campaign(env.clone(), campaign_id).unwrap();
        (campaign.fee_bps, campaign.platform_wallet)
    }

    pub fn bump_campaign_ttl(env: Env, campaign_id: u64) {
        let key = DataKey::Campaign(campaign_id);
        env.storage().persistent().extend_ttl(&key, MIN_TTL, MAX_TTL);
    }

    fn ensure_admin(env: &Env, admin: &Address) {
        let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if stored_admin != *admin {
            panic!("unauthorized");
        }
    }

    fn next_campaign_id(env: &Env) -> u64 {
        let mut next_id: u64 = env.storage().instance().get(&DataKey::CampaignCount).unwrap_or(0_u64);
        next_id += 1;
        env.storage().instance().set(&DataKey::CampaignCount, &next_id);
        next_id
    }
}

#[cfg(test)]
mod invariant_tests;
#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    #[test]
    fn campaign_admin_and_status_flow() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, CampaignContract);
        let client = CampaignContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let owner = Address::generate(&env);

        client.initialize(&admin);
        let campaign_id = client.create_campaign(&owner, &1_000_i128, &2_000_u64, &500, &None);
        let campaign = client.get_campaign(&campaign_id).unwrap();

        assert_eq!(campaign.owner, owner);
        assert_eq!(campaign.goal, 1_000_i128);
        assert_eq!(campaign.status, CampaignStatus::Active);
        assert_eq!(campaign.fee_bps, 500);
        assert_eq!(campaign.platform_wallet, None);
        assert_eq!(client.get_campaign_count(), 1_u64);

        client.suspend_campaign(&admin, &campaign_id);
        let suspended = client.get_campaign(&campaign_id).unwrap();
        assert_eq!(suspended.status, CampaignStatus::Suspended);

        client.reject_campaign(&admin, &campaign_id, &String::from_str(&env, "spam"));
        let rejected = client.get_campaign(&campaign_id).unwrap();
        assert_eq!(rejected.status, CampaignStatus::Rejected);
    }

    #[test]
    fn pause_blocks_state_mutations() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, CampaignContract);
        let client = CampaignContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let owner = Address::generate(&env);

        client.initialize(&admin);
        client.pause(&admin);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.create_campaign(&owner, &1_000_i128, &2_000_u64, &500, &None);
        }));
        assert!(result.is_err());

        client.unpause(&admin);
        let campaign_id = client.create_campaign(&owner, &1_000_i128, &2_000_u64, &500, &None);
        assert_eq!(campaign_id, 1);
    }
}