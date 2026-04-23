#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, vec, Address, Env, String, Symbol, Vec,
};

// ── Storage key helpers ──────────────────────────────────────────────────────

fn campaign_key(id: u64) -> (Symbol, u64) {
    (symbol_short!("camp"), id)
}

fn donors_key(campaign_id: u64) -> (Symbol, u64) {
    (symbol_short!("donors"), campaign_id)
}

fn donation_key(campaign_id: u64, donor: &Address) -> (Symbol, u64, Address) {
    (symbol_short!("don"), campaign_id, donor.clone())
}

// ── Data types ───────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Campaign {
    pub id: u64,
    pub creator: Address,
    pub title: Symbol,
    pub goal: i128,
    pub raised: i128,
    pub deadline: u64,
    pub active: bool,
}

/// Issue #100 – donation metadata: memo, donor public key, timestamp
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DonationMetadata {
    pub campaign_id: u64,
    pub donor: Address,
    pub amount: i128,
    pub memo: String,
    pub timestamp: u64,
}

// ── Contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct StellarAidContract;

#[contractimpl]
impl StellarAidContract {
    /// Initialize the contract with admin address
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        env.storage().instance().set(&symbol_short!("admin"), &admin);
        env.storage().instance().set(&symbol_short!("count"), &0u64);
    }

    /// Ping method for health check
    pub fn ping() -> u32 {
        1
    }

    /// Create a new campaign
    pub fn create_campaign(
        env: Env,
        creator: Address,
        title: Symbol,
        goal: i128,
        deadline: u64,
    ) -> u64 {
        creator.require_auth();

        let mut count: u64 = env
            .storage()
            .instance()
            .get(&symbol_short!("count"))
            .unwrap_or(0);

        count += 1;

        let campaign = Campaign {
            id: count,
            creator,
            title,
            goal,
            raised: 0,
            deadline,
            active: true,
        };

        // Issue #99 – store each campaign keyed by its ID
        env.storage().persistent().set(&campaign_key(count), &campaign);
        env.storage().instance().set(&symbol_short!("count"), &count);

        count
    }

    /// Donate to a campaign
    /// Issue #99  – validates campaign existence and stores donation→campaign mapping
    /// Issue #100 – stores DonationMetadata (memo, donor key, timestamp)
    /// Issue #101 – tracks unique donors per campaign
    pub fn donate(env: Env, donor: Address, campaign_id: u64, amount: i128, memo: String) {
        donor.require_auth();

        // Issue #99 – validate campaign existence
        let mut campaign: Campaign = env
            .storage()
            .persistent()
            .get(&campaign_key(campaign_id))
            .expect("Campaign not found");

        assert!(campaign.active, "Campaign is not active");
        assert!(amount > 0, "Amount must be greater than 0");

        campaign.raised += amount;
        env.storage().persistent().set(&campaign_key(campaign_id), &campaign);

        // Issue #100 – store donation metadata
        let metadata = DonationMetadata {
            campaign_id,
            donor: donor.clone(),
            amount,
            memo,
            timestamp: env.ledger().timestamp(),
        };
        env.storage()
            .persistent()
            .set(&donation_key(campaign_id, &donor), &metadata);

        // Issue #101 – maintain unique donor list per campaign
        let mut donors: Vec<Address> = env
            .storage()
            .persistent()
            .get(&donors_key(campaign_id))
            .unwrap_or_else(|| vec![&env]);

        if !donors.contains(&donor) {
            donors.push_back(donor);
            env.storage().persistent().set(&donors_key(campaign_id), &donors);
        }
    }

    /// Get campaign details
    pub fn get_campaign(env: Env, campaign_id: u64) -> Option<Campaign> {
        env.storage().persistent().get(&campaign_key(campaign_id))
    }

    /// Issue #101 – get donor list for a campaign
    pub fn get_donors(env: Env, campaign_id: u64) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&donors_key(campaign_id))
            .unwrap_or_else(|| vec![&env])
    }

    /// Issue #101 – get unique donor count for a campaign
    pub fn get_donor_count(env: Env, campaign_id: u64) -> u32 {
        let donors: Vec<Address> = env
            .storage()
            .persistent()
            .get(&donors_key(campaign_id))
            .unwrap_or_else(|| vec![&env]);
        donors.len()
    }

    /// Issue #100 – get donation metadata for a specific donor + campaign
    pub fn get_donation(env: Env, campaign_id: u64, donor: Address) -> Option<DonationMetadata> {
        env.storage()
            .persistent()
            .get(&donation_key(campaign_id, &donor))
    }

    /// Get admin address
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&symbol_short!("admin"))
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, String};

    #[test]
    fn test_ping() {
        let env = Env::default();
        let contract_id = env.register_contract(None, StellarAidContract);
        let client = StellarAidContractClient::new(&env, &contract_id);
        assert_eq!(client.ping(), 1);
    }

    #[test]
    fn test_initialize() {
        let env = Env::default();
        let contract_id = env.register_contract(None, StellarAidContract);
        let client = StellarAidContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);
        assert_eq!(client.get_admin(), Some(admin));
    }

    #[test]
    fn test_create_and_donate_with_metadata_and_tracking() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, StellarAidContract);
        let client = StellarAidContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let creator = Address::generate(&env);
        let cid = client.create_campaign(
            &creator,
            &symbol_short!("test"),
            &1000,
            &9999999,
        );
        assert_eq!(cid, 1);

        let donor1 = Address::generate(&env);
        let donor2 = Address::generate(&env);
        let memo = String::from_str(&env, "donation memo");

        client.donate(&donor1, &cid, &100, &memo);
        client.donate(&donor2, &cid, &200, &memo);
        // donor1 donates again – should not increase unique count
        client.donate(&donor1, &cid, &50, &memo);

        // #99 – campaign raised updated
        let campaign = client.get_campaign(&cid).unwrap();
        assert_eq!(campaign.raised, 350);

        // #101 – unique donor count = 2
        assert_eq!(client.get_donor_count(&cid), 2);

        // #100 – metadata stored
        let meta = client.get_donation(&cid, &donor1).unwrap();
        assert_eq!(meta.donor, donor1);
        assert_eq!(meta.campaign_id, cid);
    }
}
