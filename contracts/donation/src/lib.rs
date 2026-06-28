#![no_std]

use soroban_sdk::{contract, contractclient, contractimpl, contracttype, Address, BytesN, Env, Symbol, Vec};
use shared::pause;
use shared::types::{Campaign, CampaignStatus, Donation};

#[contractclient(name = "CampaignContractClient")]
trait CampaignContractTrait {
    fn update_raised(env: Env, campaign_id: u64, amount: i128);
    fn get_campaign(env: Env, campaign_id: u64) -> Option<Campaign>;
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin = 0,
    DonationHistory(Address) = 1,
    CampaignDonations(u64) = 2,
    CampaignRaised(u64) = 3,
    CampaignContract = 4,
    Initialized = 5,
}

#[contracttype]
#[derive(Clone)]
pub struct DonationMadeEvent {
    pub donor: Address,
    pub campaign_id: u64,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone)]
pub struct RefundRecordedEvent {
    pub campaign_id: u64,
    pub donor: Address,
    pub amount: i128,
    pub caller: Address,
}

#[contract]
pub struct DonationContract;

#[contractimpl]
impl DonationContract {
    /// Initialize the donation contract with an admin and campaign contract address.
    /// Must be called once before any other operations.
    pub fn initialize(env: Env, admin: Address, campaign_contract: Address) {
        admin.require_auth();
        if env.storage().instance().has(&DataKey::Initialized) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::CampaignContract, &campaign_contract);
        env.storage().instance().set(&DataKey::Initialized, &true);
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

    /// Make a donation to a campaign.
    /// Verifies the campaign exists and is active, records the donation,
    /// updates the raised total, and calls the campaign contract to
    /// increment its raised amount via cross-contract call.
    pub fn donate(env: Env, donor: Address, campaign_id: u64, amount: i128) {
        pause::require_not_paused(&env);
        donor.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }

        let campaign_contract: Address = env.storage().instance().get(&DataKey::CampaignContract).unwrap();
        let campaign_client = CampaignContractClient::new(&env, &campaign_contract);
        let campaign = campaign_client.get_campaign(&campaign_id).unwrap_or_else(|| panic!("campaign not found"));
        if campaign.status != CampaignStatus::Active {
            panic!("campaign is not active");
        }

        let mut donations = env.storage().persistent().get(&DataKey::CampaignDonations(campaign_id)).unwrap_or(Vec::new(&env));
        let timestamp = env.ledger().timestamp();
        let donation = Donation {
            donor: donor.clone(),
            campaign_id,
            amount,
            timestamp,
        };
        donations.push_back(donation.clone());
        env.storage().persistent().set(&DataKey::CampaignDonations(campaign_id), &donations);

        let donor_history = env.storage().persistent().get(&DataKey::DonationHistory(donor.clone())).unwrap_or(Vec::new(&env));
        let mut history = donor_history;
        history.push_back(donation.clone());
        env.storage().persistent().set(&DataKey::DonationHistory(donor), &history);

        let total = env.storage().persistent().get(&DataKey::CampaignRaised(campaign_id)).unwrap_or(0_i128);
        env.storage().persistent().set(&DataKey::CampaignRaised(campaign_id), &(total + amount));

        campaign_client.update_raised(&campaign_id, &amount);

        env.events().publish((Symbol::new(&env, "donation_made"),), DonationMadeEvent {
            donor: donation.donor,
            campaign_id,
            amount,
        });
    }

    /// Issue a refund to a donor for a specific campaign.
    /// Only the admin or the campaign owner can authorize refunds.
    pub fn refund(env: Env, caller: Address, campaign_id: u64, donor: Address, amount: i128) {
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        let campaign_contract: Address = env.storage().instance().get(&DataKey::CampaignContract).unwrap();
        let campaign_client = CampaignContractClient::new(&env, &campaign_contract);
        let campaign = campaign_client.get_campaign(&campaign_id).unwrap_or_else(|| panic!("campaign not found"));
        if caller != admin && caller != campaign.owner {
            panic!("unauthorized");
        }
        let total = env.storage().persistent().get(&DataKey::CampaignRaised(campaign_id)).unwrap_or(0_i128);
        env.storage().persistent().set(&DataKey::CampaignRaised(campaign_id), &(total - amount));
        env.events().publish((Symbol::new(&env, "refund_recorded"),), RefundRecordedEvent {
            campaign_id,
            donor,
            amount,
            caller,
        });
    }

    /// Return all donations made to a given campaign.
    pub fn get_donations_for_campaign(env: Env, campaign_id: u64) -> Vec<Donation> {
        env.storage().persistent().get(&DataKey::CampaignDonations(campaign_id)).unwrap_or(Vec::new(&env))
    }

    /// Return the total amount raised for a given campaign (tracked locally).
    pub fn get_total_raised(env: Env, campaign_id: u64) -> i128 {
        env.storage().persistent().get(&DataKey::CampaignRaised(campaign_id)).unwrap_or(0_i128)
    }

    /// Return the donation history for a specific donor.
    pub fn get_donor_history(env: Env, donor: Address) -> Vec<Donation> {
        env.storage().persistent().get(&DataKey::DonationHistory(donor)).unwrap_or(Vec::new(&env))
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
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    #[test]
    fn donation_flow_records_history_and_total() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);
        let donor = Address::generate(&env);
        let admin = Address::generate(&env);
        let campaign_contract = Address::generate(&env);

        client.initialize(&admin, &campaign_contract);
        client.donate(&donor, &7_u64, &100_i128);

        let donations = client.get_donations_for_campaign(&7_u64);
        assert_eq!(donations.len(), 1);
        assert_eq!(client.get_total_raised(&7_u64), 100_i128);

        let history = client.get_donor_history(&donor);
        assert_eq!(history.len(), 1);
        assert_eq!(history.get(0).unwrap().amount, 100_i128);
    }

    #[test]
    fn pause_blocks_donations() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);
        let donor = Address::generate(&env);
        let admin = Address::generate(&env);
        let campaign_contract = Address::generate(&env);

        client.initialize(&admin, &campaign_contract);
        client.pause(&admin);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.donate(&donor, &7_u64, &100_i128);
        }));
        assert!(result.is_err());

        client.unpause(&admin);
        client.donate(&donor, &7_u64, &100_i128);
        assert_eq!(client.get_total_raised(&7_u64), 100_i128);
    }
}
