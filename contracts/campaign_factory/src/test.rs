#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{CampaignFactory, CampaignFactoryClient};

#[test]
fn registry_tracks_deployments() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CampaignFactory);
    let client = CampaignFactoryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let contract_a = Address::generate(&env);
    let contract_b = Address::generate(&env);

    client.initialize(&admin).unwrap();
    client.deploy_campaign(&creator, &contract_a).unwrap();
    client.register_campaign(&creator, &contract_b).unwrap();

    let all = client.get_all_campaigns();
    assert_eq!(all.len(), 2);
    assert_eq!(client.get_campaigns_by_creator(&creator).len(), 2);

    let by_creator = client.get_campaigns_by_creator(&creator);
    let mut found_a = false;
    let mut found_b = false;
    for pair in by_creator.iter() {
        let (pair_creator, pair_contract) = pair;
        if pair_creator == creator && pair_contract == contract_a {
            found_a = true;
        }
        if pair_creator == creator && pair_contract == contract_b {
            found_b = true;
        }
    }
    assert!(found_a && found_b);
}

#[test]
fn duplicate_registration_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CampaignFactory);
    let client = CampaignFactoryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let contract = Address::generate(&env);

    client.initialize(&admin).unwrap();
    client.deploy_campaign(&creator, &contract).unwrap();
    let result = client.try_deploy_campaign(&creator, &contract);
    assert!(result.is_err());
}
