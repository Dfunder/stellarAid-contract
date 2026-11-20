#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl, testutils::Address as _, Address, Bytes, BytesN, Env, Symbol, Vec,
};

use crate::{CampaignFactory, CampaignFactoryClient, CampaignFactoryError, DataKey};

#[contract]
pub struct TestCampaign;

#[contractimpl]
impl TestCampaign {
    pub fn initialize(env: Env, admin: Address) {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "admin"), &admin);
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "admin"))
            .unwrap()
    }
}

fn initialized_factory(env: &Env) -> (CampaignFactoryClient, Address, Address) {
    env.mock_all_auths();
    let factory_id = env.register_contract(None, CampaignFactory);
    let client = CampaignFactoryClient::new(env, &factory_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, factory_id, admin)
}

fn seed_registry(env: &Env, factory: &Address, campaigns: &Vec<Address>) {
    env.as_contract(factory, || {
        env.storage().instance().set(&DataKey::Campaigns, campaigns);
    });
}

fn mock_campaigns(env: &Env, admins: &Vec<Address>) -> Vec<Address> {
    let mut out = Vec::new(env);
    for admin in admins {
        let id = env.register_contract(None, TestCampaign);
        TestCampaignClient::new(env, &id).initialize(&admin);
        out.push_back(id);
    }
    out
}

#[test]
fn error_codes_are_stable() {
    assert_eq!(CampaignFactoryError::AlreadyInitialized as u32, 1);
    assert_eq!(CampaignFactoryError::NotInitialized as u32, 2);
    assert_eq!(CampaignFactoryError::WasmHashNotSet as u32, 3);
    assert_eq!(CampaignFactoryError::Unauthorized as u32, 4);
    assert_eq!(
        shared::errors::SharedErrorCode::CampaignNotFound as u32,
        300
    );
}

#[test]
fn initialize_rejects_second_init() {
    let env = Env::default();
    let (client, _factory_id, admin) = initialized_factory(&env);
    let result = client.try_initialize(&admin);
    assert!(result.is_err());
}

#[test]
fn wasm_hash_roundtrip_and_deploy_requires_it() {
    let env = Env::default();
    let (client, _factory_id, admin) = initialized_factory(&env);
    let creator = Address::generate(&env);
    let params = Bytes::from_slice(&env, &[1, 2, 3, 4]);

    assert_eq!(client.get_wasm_hash(), None);
    let err = client
        .try_deploy_campaign(&creator, &params)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, CampaignFactoryError::WasmHashNotSet);

    let hash = BytesN::from_array(&env, &[7u8; 32]);
    client.update_wasm_hash(&admin, &hash);
    assert_eq!(client.get_wasm_hash(), Some(hash));
}

#[test]
fn get_all_campaigns_page_walks_the_registry_in_pages() {
    let env = Env::default();
    let (client, factory_id, _admin) = initialized_factory(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let mut admins = Vec::new(&env);
    for _ in 0..5 {
        admins.push_back(a.clone());
    }
    for _ in 0..2 {
        admins.push_back(b.clone());
    }
    let campaigns = mock_campaigns(&env, &admins);
    seed_registry(&env, &factory_id, &campaigns);

    let (page, info) = client.get_all_campaigns_page(&0, &3);
    assert_eq!(page.len(), 3);
    assert_eq!(info.total, 7);
    assert_eq!(info.count, 3);
    assert!(info.has_more);
    assert_eq!(info.next_start, Some(3));

    let (page, info) = client.get_all_campaigns_page(&3, &3);
    assert_eq!(page.len(), 3);
    assert!(info.has_more);
    assert_eq!(info.next_start, Some(6));

    let (page, info) = client.get_all_campaigns_page(&6, &3);
    assert_eq!(page.len(), 1);
    assert!(!info.has_more);
    assert_eq!(info.next_start, None);

    let mut seen = 0;
    let mut start = 0u32;
    loop {
        let (page, info) = client.get_all_campaigns_page(&start, &3);
        seen += page.len() as u32;
        match info.next_start {
            Some(next) => {
                assert_eq!(next, start + 3);
                start = next;
            }
            None => break,
        }
    }
    assert_eq!(seen, 7);
    assert_eq!(client.get_all_campaigns().len(), 7);
}

#[test]
fn get_all_campaigns_page_handles_the_edge_cases() {
    let env = Env::default();
    let (client, factory_id, _admin) = initialized_factory(&env);
    let a = Address::generate(&env);
    let campaigns = mock_campaigns(&env, &Vec::from_slice(&env, &[a.clone(), a.clone()]));
    seed_registry(&env, &factory_id, &campaigns);

    let (page, info) = client.get_all_campaigns_page(&99, &10);
    assert!(page.is_empty());
    assert_eq!(info.total, 2);
    assert_eq!(info.count, 0);
    assert!(!info.has_more);
    assert_eq!(info.next_start, None);

    let (page, info) = client.get_all_campaigns_page(&0, &0);
    assert_eq!(info.limit, 2);
    assert_eq!(page.len(), 2);

    let (page, info) = client.get_all_campaigns_page(&0, &(shared::pagination::MAX_LIMIT + 1));
    assert_eq!(info.limit, 2);
    assert_eq!(page.len(), 2);

    let (page, info) = client.get_all_campaigns_page(&u32::MAX, &u32::MAX);
    assert!(page.is_empty());
    assert!(!info.has_more);
}

#[test]
fn get_campaigns_by_creator_page_filters_then_pages() {
    let env = Env::default();
    let (client, factory_id, _admin) = initialized_factory(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let mut admins = Vec::new(&env);
    for _ in 0..5 {
        admins.push_back(a.clone());
    }
    for _ in 0..2 {
        admins.push_back(b.clone());
    }
    let campaigns = mock_campaigns(&env, &admins);
    seed_registry(&env, &factory_id, &campaigns);

    let (page, info) = client.get_campaigns_by_creator_page(&a, &0, &2);
    assert_eq!(page.len(), 2);
    assert_eq!(info.total, 5);
    assert!(info.has_more);
    assert_eq!(info.next_start, Some(2));

    let (page, info) = client.get_campaigns_by_creator_page(&a, &4, &2);
    assert_eq!(page.len(), 1);
    assert!(!info.has_more);
    assert_eq!(info.next_start, None);

    let (page, info) = client.get_campaigns_by_creator_page(&b, &0, &10);
    assert_eq!(page.len(), 2);
    assert_eq!(info.total, 2);
    assert!(!info.has_more);

    let (page, info) = client.get_campaigns_by_creator_page(&Address::generate(&env), &0, &10);
    assert!(page.is_empty());
    assert_eq!(info.total, 0);
    assert!(!info.has_more);
    assert_eq!(info.next_start, None);

    assert_eq!(client.get_campaigns_by_creator(&a).len(), 5);
    assert_eq!(client.get_campaigns_by_creator(&b).len(), 2);
}
