use crate::types::AddressEnvironment;
use crate::types::ResolutionCacheEntry;
use crate::PlatformConfigContractClient;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::{symbol_short, Address, Env, Symbol};

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

// ── Address registry + resolution caching (#662) ────────────────────────────

const ESCROW: Symbol = symbol_short!("escrow");

#[test]
fn register_and_resolve_address_roundtrips() {
    let env = Env::default();
    let (client, _admin, _, _) = setup(&env);
    let escrow_addr = Address::generate(&env);
    client.register_address(&AddressEnvironment::Production, &ESCROW, &escrow_addr);
    let resolved = client.resolve_address(&AddressEnvironment::Production, &ESCROW);
    assert_eq!(resolved, escrow_addr);
}

#[test]
fn test_and_production_namespaces_are_independent() {
    let env = Env::default();
    let (client, _admin, _, _) = setup(&env);
    let prod = Address::generate(&env);
    let test = Address::generate(&env);
    client.register_address(&AddressEnvironment::Production, &ESCROW, &prod);
    client.register_address(&AddressEnvironment::Test, &ESCROW, &test);
    assert_eq!(
        client.resolve_address(&AddressEnvironment::Production, &ESCROW),
        prod
    );
    assert_eq!(
        client.resolve_address(&AddressEnvironment::Test, &ESCROW),
        test
    );
    assert_eq!(client.get_registered_address(&AddressEnvironment::Production, &ESCROW), prod);
}

#[test]
fn resolve_unregistered_address_fails() {
    let env = Env::default();
    let (client, _admin, _, _) = setup(&env);
    let err = client
        .try_resolve_address(&AddressEnvironment::Production, &ESCROW)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, crate::ConfigError::AddressNotRegistered);
}

#[test]
fn unregister_address_removes_entry() {
    let env = Env::default();
    let (client, _admin, _, _) = setup(&env);
    let escrow_addr = Address::generate(&env);
    client.register_address(&AddressEnvironment::Production, &ESCROW, &escrow_addr);
    client.unregister_address(&AddressEnvironment::Production, &ESCROW);
    let err = client
        .try_resolve_address(&AddressEnvironment::Production, &ESCROW)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, crate::ConfigError::AddressNotRegistered);
}

#[test]
fn register_overwrites_existing_address() {
    let env = Env::default();
    let (client, _admin, _, _) = setup(&env);
    let v1 = Address::generate(&env);
    let v2 = Address::generate(&env);
    client.register_address(&AddressEnvironment::Production, &ESCROW, &v1);
    client.register_address(&AddressEnvironment::Production, &ESCROW, &v2);
    assert_eq!(client.resolve_address(&AddressEnvironment::Production, &ESCROW), v2);
}

#[test]
fn resolve_populates_the_resolution_cache() {
    let env = Env::default();
    let (client, _admin, _, _) = setup(&env);
    let escrow_addr = Address::generate(&env);
    client.register_address(&AddressEnvironment::Production, &ESCROW, &escrow_addr);
    client.resolve_address(&AddressEnvironment::Production, &ESCROW);
    let cached: ResolutionCacheEntry =
        client.resolution_cache(&AddressEnvironment::Production, &ESCROW);
    assert_eq!(cached.address, escrow_addr);
    assert_eq!(cached.resolved_ledger, env.ledger().sequence());
}

#[test]
fn re_register_invalidates_the_cache() {
    let env = Env::default();
    let (client, _admin, _, _) = setup(&env);
    let v1 = Address::generate(&env);
    let v2 = Address::generate(&env);
    client.register_address(&AddressEnvironment::Production, &ESCROW, &v1);
    client.resolve_address(&AddressEnvironment::Production, &ESCROW);
    client.register_address(&AddressEnvironment::Production, &ESCROW, &v2);
    let resolved = client.resolve_address(&AddressEnvironment::Production, &ESCROW);
    assert_eq!(resolved, v2, "re-register must not be masked by a stale cache");
}

#[test]
fn stale_cache_is_refreshed_from_the_registry() {
    let env = Env::default();
    // Move the ledger forward so a stamp of 0 looks stale against the TTL.
    env.ledger().set_sequence_number(crate::storage::RESOLUTION_CACHE_TTL_LEDGERS + 10);
    let (client, _admin, _, _) = setup(&env);
    let escrow_addr = Address::generate(&env);
    client.register_address(&AddressEnvironment::Production, &ESCROW, &escrow_addr);
    client.resolve_address(&AddressEnvironment::Production, &ESCROW);

    // Forge an outdated cache stamp to exercise the staleness refresh branch.
    env.as_contract(&client.address, || {
        env.storage().instance().set(
            &crate::storage::DataKey::ResolutionCache(AddressEnvironment::Production, ESCROW),
            &ResolutionCacheEntry {
                address: escrow_addr.clone(),
                resolved_ledger: 0,
            },
        );
    });

    let cached: ResolutionCacheEntry =
        client.resolution_cache(&AddressEnvironment::Production, &ESCROW);
    assert!(
        env.ledger().sequence() - cached.resolved_ledger > crate::storage::RESOLUTION_CACHE_TTL_LEDGERS
    );

    // The re-resolve returns the same address (still registered) and refreshes the stamp.
    assert_eq!(client.resolve_address(&AddressEnvironment::Production, &ESCROW), escrow_addr);
    let fresh: ResolutionCacheEntry =
        client.resolution_cache(&AddressEnvironment::Production, &ESCROW);
    assert_eq!(fresh.resolved_ledger, env.ledger().sequence());
}

#[test]
fn active_environment_resolution_switches_with_set_environment() {
    let env = Env::default();
    let (client, _admin, _, _) = setup(&env);
    let prod = Address::generate(&env);
    let test = Address::generate(&env);
    client.register_address(&AddressEnvironment::Production, &ESCROW, &prod);
    client.register_address(&AddressEnvironment::Test, &ESCROW, &test);

    assert_eq!(client.resolve_for_environment(&ESCROW), prod);
    client.set_environment(&AddressEnvironment::Test);
    assert_eq!(client.get_environment(), AddressEnvironment::Test);
    assert_eq!(client.resolve_for_environment(&ESCROW), test);
}

#[test]
fn registry_entry_returns_full_record() {
    let env = Env::default();
    let (client, _admin, _, _) = setup(&env);
    let escrow_addr = Address::generate(&env);
    client.register_address(&AddressEnvironment::Test, &ESCROW, &escrow_addr);
    let entry = client.registry_entry(&AddressEnvironment::Test, &ESCROW);
    assert_eq!(entry.env, AddressEnvironment::Test);
    assert_eq!(entry.name, ESCROW);
    assert_eq!(entry.address, escrow_addr);
}
