#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, Symbol};

#[contract]
pub struct StellarAidContract;

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
            creator: creator.clone(),
            title,
            goal,
            raised: 0,
            deadline,
            active: true,
        };

        env.storage()
            .instance()
            .set(&symbol_short!("camp_"), &campaign);
        env.storage().instance().set(&symbol_short!("count"), &count);

        count
    }

    /// Donate to a campaign
    pub fn donate(env: Env, donor: Address, campaign_id: u64, amount: i128) {
        donor.require_auth();

        let mut campaign: Campaign = env
            .storage()
            .instance()
            .get(&symbol_short!("camp_"))
            .expect("Campaign not found");

        assert!(campaign.active, "Campaign is not active");
        assert!(amount > 0, "Amount must be greater than 0");

        campaign.raised += amount;

        env.storage()
            .instance()
            .set(&symbol_short!("camp_"), &campaign);
    }

    /// Get campaign details
    pub fn get_campaign(env: Env, campaign_id: u64) -> Option<Campaign> {
        env.storage().instance().get(&symbol_short!("camp_"))
    }

    /// Get admin address
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&symbol_short!("admin"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

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

        let stored_admin = client.get_admin();
        assert_eq!(stored_admin, Some(admin));
    }
}
