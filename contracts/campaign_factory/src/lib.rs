#![no_std]

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, Vec};

include!("../../semver_types.rs");

#[contracttype]
pub enum DataKey {
    Admin,
    Initialized,
    Campaigns,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum CampaignFactoryError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    ContractAlreadyRegistered = 3,
}

fn read_campaigns(env: &Env) -> Vec<(Address, Address)> {
    env.storage()
        .instance()
        .get(&DataKey::Campaigns)
        .unwrap_or_else(|| Vec::new(env))
}

#[contract]
pub struct CampaignFactory;

#[contractimpl]
impl CampaignFactory {
    pub fn initialize(env: Env, admin: Address) -> Result<(), CampaignFactoryError> {
        if env.storage().instance().has(&DataKey::Initialized) {
            return Err(CampaignFactoryError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage()
            .instance()
            .set(&DataKey::Campaigns, &Vec::<(Address, Address)>::new(&env));
        Ok(())
    }

    impl_semver_queries!();

    pub fn deploy_campaign(
        env: Env,
        creator: Address,
        contract: Address,
    ) -> Result<(), CampaignFactoryError> {
        Self::register_campaign(env, creator, contract)
    }

    pub fn register_campaign(
        env: Env,
        creator: Address,
        contract: Address,
    ) -> Result<(), CampaignFactoryError> {
        if !env.storage().instance().has(&DataKey::Initialized) {
            return Err(CampaignFactoryError::NotInitialized);
        }
        creator.require_auth();

        let mut campaigns = read_campaigns(&env);
        for pair in campaigns.iter() {
            let (existing_creator, existing_contract) = pair;
            if existing_creator == creator && existing_contract == contract {
                return Err(CampaignFactoryError::ContractAlreadyRegistered);
            }
        }

        campaigns.push_back((creator, contract));
        env.storage().instance().set(&DataKey::Campaigns, &campaigns);
        Ok(())
    }

    pub fn get_all_campaigns(env: Env) -> Vec<(Address, Address)> {
        read_campaigns(&env)
    }

    pub fn get_campaigns_by_creator(env: Env, creator: Address) -> Vec<(Address, Address)> {
        let campaigns = read_campaigns(&env);
        let mut filtered = Vec::new(&env);
        for pair in campaigns.iter() {
            let (existing_creator, contract) = pair;
            if existing_creator == creator {
                filtered.push_back((existing_creator, contract));
            }
        }
        filtered
    }
}
