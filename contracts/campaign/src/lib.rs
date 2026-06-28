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
    pub fn create_campaign(env: Env, owner: Address, goal: i128, deadline: u64) -> u64 {
        pause::require_not_paused(&env);
        owner.require_auth();
        let id = Self::next_campaign_id(&env);
        let campaign = Campaign {
            id,
            owner: owner.clone(),
            goal,
            raised: 0,
            status: CampaignStatus::Active,
            deadline,
        };
        env.storage().persistent().set(&DataKey::Campaign(id), &campaign);
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
    }

    /// Approve a campaign, moving it to Active status.
    pub fn approve_campaign(env: Env, admin: Address, campaign_id: u64) {
        Self::update_campaign_status(env, admin, campaign_id, CampaignStatus::Active);
    }

    /// Reject a campaign, moving it to Rejected status.
    pub fn reject_campaign(env: Env, admin: Address, campaign_id: u64, reason: String) {
        pause::require_not_paused(&env);
        admin.require_auth();
        Self::ensure_admin(&env, &admin);
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
        let campaign_id = client.create_campaign(&owner, &1_000_i128, &2_000_u64);
        let campaign = client.get_campaign(&campaign_id).unwrap();

        assert_eq!(campaign.owner, owner);
        assert_eq!(campaign.goal, 1_000_i128);
        assert_eq!(campaign.status, CampaignStatus::Active);
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
            client.create_campaign(&owner, &1_000_i128, &2_000_u64);
        }));
        assert!(result.is_err());

        client.unpause(&admin);
        let campaign_id = client.create_campaign(&owner, &1_000_i128, &2_000_u64);
        assert_eq!(campaign_id, 1);
    }
}
