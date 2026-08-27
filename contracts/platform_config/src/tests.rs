use crate::PlatformConfigContractClient;
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup(env: &Env) -> (crate::PlatformConfigContractClient<'_>, Address, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register_contract(None, crate::PlatformConfigContract);
    let client = PlatformConfigContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let wallet = Address::generate(env);
    let token = Address::generate(env);
    client.initialize(&admin, &500, &wallet, &token);
    (client, admin, wallet, token)
}

#[test]
fn test_set_fee_bps_success() {
    let env = Env::default();
    let (client, _, _, _) = setup(&env);
    client.set_fee_bps(&200);
    assert_eq!(client.get_config().fee_bps, 200);
}

#[test]
fn test_initialize_success() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, crate::PlatformConfigContract);
    let client = PlatformConfigContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let wallet = Address::generate(&env);
    let token = Address::generate(&env);
    client.initialize(&admin, &500, &wallet, &token);
    let config = client.get_config();
    assert_eq!(config.admin, admin);
    assert_eq!(config.fee_bps, 500);
}

#[test]
#[should_panic]
fn test_set_fee_bps_too_high() {
    let env = Env::default();
    let (client, _, _, _) = setup(&env);
    client.set_fee_bps(&1001);
}

#[test]
fn test_transfer_admin_sets_pending() {
    let env = Env::default();
    let (client, _admin, _, _) = setup(&env);
    let new_admin = Address::generate(&env);
    client.transfer_admin(&new_admin);
    client.accept_admin();
    assert_eq!(client.get_config().admin, new_admin);
}

#[test]
fn test_accept_admin_updates_admin() {
    let env = Env::default();
    let (client, _old_admin, _, _) = setup(&env);
    let new_admin = Address::generate(&env);
    client.transfer_admin(&new_admin);
    client.accept_admin();
    let config = client.get_config();
    assert_eq!(config.admin, new_admin);
}

#[test]
#[should_panic]
fn test_initialize_already_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, crate::PlatformConfigContract);
    let client = PlatformConfigContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let wallet = Address::generate(&env);
    let token = Address::generate(&env);
    client.initialize(&admin, &500, &wallet, &token);
    client.initialize(&admin, &500, &wallet, &token);
}

#[test]
#[should_panic]
fn test_initialize_fee_bps_too_high() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, crate::PlatformConfigContract);
    let client = PlatformConfigContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let wallet = Address::generate(&env);
    let token = Address::generate(&env);
    client.initialize(&admin, &1001, &wallet, &token);
}

#[test]
fn test_get_config_returns_correct_values() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, crate::PlatformConfigContract);
    let client = PlatformConfigContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let wallet = Address::generate(&env);
    let token = Address::generate(&env);
    client.initialize(&admin, &250, &wallet, &token);
    let config = client.get_config();
    assert_eq!(config.fee_bps, 250);
    assert_eq!(config.platform_wallet, wallet);
    assert_eq!(config.usdc_token, token);
}

#[test]
fn health_check_is_healthy_after_init() {
    let env = Env::default();
    let (client, admin, _, _) = setup(&env);
    let report = client.health_check();
    assert_eq!(report.status, shared::HealthStatus::Healthy);
    client.report_ok(&admin);
    assert_eq!(client.get_health_metrics().ok_count, 1);
}
