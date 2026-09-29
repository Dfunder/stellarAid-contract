#![no_std]

use campaign::{CampaignContract, CampaignContractClient};
use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Bytes, Env, Symbol, Vec};

include!("../../semver_types.rs");

#[contracttype]
pub enum DataKey {
    Admin,
    Initialized,
    Campaigns,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignDeployedEvent {
    pub creator: Address,
    pub contract: Address,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum CampaignFactoryError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
}

fn read_campaigns(env: &Env) -> Vec<Address> {
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
            .set(&DataKey::Campaigns, &Vec::<Address>::new(&env));
        Ok(())
    }

    impl_semver_queries!();

    pub fn deploy_campaign(
        env: Env,
        creator: Address,
        _params: Bytes,
    ) -> Result<Address, CampaignFactoryError> {
        if !env.storage().instance().has(&DataKey::Initialized) {
            return Err(CampaignFactoryError::NotInitialized);
        }

        creator.require_auth();

        let contract_id = env.register_contract(None, CampaignContract);
        let client = CampaignContractClient::new(&env, &contract_id);
        client.initialize(&creator);

        let mut campaigns = read_campaigns(&env);
        campaigns.push_back(contract_id.clone());
        env.storage().instance().set(&DataKey::Campaigns, &campaigns);

        env.events().publish(
            (Symbol::new(&env, "campaign_deployed"),),
            CampaignDeployedEvent {
                creator: creator.clone(),
                contract: contract_id.clone(),
            },
        );

        Ok(contract_id)
    }

    pub fn get_all_campaigns(env: Env) -> Vec<Address> {
        read_campaigns(&env)
    }

    pub fn get_campaigns_by_creator(env: Env, creator: Address) -> Vec<Address> {
        let campaigns = read_campaigns(&env);
        let mut filtered = Vec::new(&env);

        for campaign in campaigns.iter() {
            let contract_id = campaign;
            let client = CampaignContractClient::new(&env, &contract_id);
            if client.get_admin() == creator {
                filtered.push_back(contract_id);
            }
        }

        filtered
    }
}
