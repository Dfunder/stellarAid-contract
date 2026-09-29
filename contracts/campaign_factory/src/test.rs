#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, Symbol};

use crate::{CampaignFactory, CampaignFactoryClient};

#[test]
fn deploy_campaign_creates_a_campaign_instance() {
    let env = Env::default();
    env.mock_all_auths();

    let factory_id = env.register_contract(None, CampaignFactory);
    let client = CampaignFactoryClient::new(&env, &factory_id);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);

    client.initialize(&admin).unwrap();
    let params = Bytes::from_slice(&env, &[1, 2, 3, 4]);
    let deployed = client.deploy_campaign(&creator, &params).unwrap();

    assert!(!deployed.to_string().is_empty());
    let all = client.get_all_campaigns();
    assert_eq!(all.len(), 1);
    assert_eq!(all.get(0).unwrap(), deployed);

    let events = env.events().all();
    assert_eq!(events.len(), 1);
    let event = &events[0];
    let topic = event.topics().get(0).unwrap();
    assert_eq!(topic, Symbol::new(&env, "campaign_deployed"));
}

#[test]
fn initialize_rejects_second_init() {
    let env = Env::default();
    env.mock_all_auths();

    let factory_id = env.register_contract(None, CampaignFactory);
    let client = CampaignFactoryClient::new(&env, &factory_id);
    let admin = Address::generate(&env);

    client.initialize(&admin).unwrap();
    let result = client.try_initialize(&admin);
    assert!(result.is_err());
}
