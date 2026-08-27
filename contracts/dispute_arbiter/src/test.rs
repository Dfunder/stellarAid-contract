extern crate std;
use soroban_sdk::{
    contract, contractimpl, testutils::{Address as _, Events as _, Ledger as _},
    Address, Bytes, Env, String, IntoVal,
};

use crate::errors::DisputeError;
use crate::types::{DisputeRecord, DisputeStatus};
use soroban_sdk::{testutils::{Address as _, Ledger, Events}, Address, Bytes, Env, String};

use crate::errors::DisputeError;
use crate::types::DisputeStatus;
use crate::{DisputeArbiter, DisputeArbiterClient};

#[contract]
pub struct MockEscrow;

#[contractimpl]
impl MockEscrow {
    pub fn open_dis(_env: Env, _commission_id: Bytes, _initiator: Address) {}
    pub fn refund_cl(_env: Env, _commission_id: Bytes, _config_contract: Address) {}
    pub fn release_p(_env: Env, _commission_id: Bytes, _config_contract: Address) {}
    pub fn rel_pay(_env: Env, _commission_id: Bytes, _config_contract: Address) {}
}

#[contract]
pub struct MockConfig;

#[contractimpl]
impl MockConfig {
    pub fn get_usdc(env: Env) -> Address {
        env.register_contract(None, MockToken)
    }
}

#[contract]
pub struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn balance(_env: Env, _id: Address) -> i128 { 1000 }
    pub fn transfer(_env: Env, _from: Address, _to: Bytes, _amount: i128) {}
}

fn create_test_env() -> (Env, Address, Address, Address, Address) {
    let env = Env::default();
    let admin = Address::generate(&env);
    let escrow_contract = env.register_contract(None, MockEscrow);
    let config_contract = env.register_contract(None, MockConfig);
    let token_admin = Address::generate(&env);
    (env, admin, escrow_contract, config_contract, token_admin)
}

fn setup_initialized(
    env: &Env,
    admin: &Address,
    escrow: &Address,
    config: &Address,
    auto_resolve: u32,
) {
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(env, &arbiter);
    client.initialize(admin, escrow, config, &auto_resolve);
}

// ========== initialize ==========

#[test]
fn test_initialize_succeeds() {
    let (env, admin, escrow, config, _token) = create_test_env();
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
}

#[test]
fn test_initialize_double_init_fails() {
    let (env, admin, escrow, config, _token) = create_test_env();
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let result = client.try_initialize(&admin, &escrow, &config, &100u32);
    assert_eq!(result.unwrap_err().unwrap(), DisputeError::AlreadyInitialized);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let result = client.try_initialize(&admin, &escrow, &config, &100u32);
    assert!(result.is_err());
}

// ========== open_dispute ==========

#[test]
fn test_open_dispute_succeeds() {
    let (env, admin, escrow, config, _token) = create_test_env();
    let initiator = Address::generate(&env);
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    client.open_dispute(&commission_id, &initiator);
    let record = client.get_dispute(&commission_id);
    assert_eq!(record.status, DisputeStatus::Open);
}

#[test]
fn test_open_dispute_already_exists_fails() {
    let (env, admin, escrow, config, _token) = create_test_env();
    let initiator = Address::generate(&env);
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    client.open_dispute(&commission_id, &initiator);
    let result = client.try_open_dispute(&commission_id, &initiator);
    assert_eq!(result.unwrap_err().unwrap(), DisputeError::AlreadyResolved);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let _ = client.open_dispute(&commission_id, &initiator);
    let result = client.try_open_dispute(&commission_id, &initiator);
    assert!(result.is_err());
}

#[test]
fn test_open_dispute_not_initialized_fails() {
    let (env, _admin, _escrow, _config, _token) = create_test_env();
    let initiator = Address::generate(&env);
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let result = client.try_open_dispute(&commission_id, &initiator);
    assert_eq!(result.unwrap_err().unwrap(), DisputeError::NotInitialized);
    assert!(result.is_err());
}

// ========== resolve_for_client ==========

#[test]
#[ignore = "requires a live escrow contract"]
fn test_resolve_for_client_succeeds() {
    let (env, admin, escrow, config, _token) = create_test_env();
    let initiator = Address::generate(&env);
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    client.open_dispute(&commission_id, &initiator);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let _ = client.open_dispute(&commission_id, &initiator);
    client.resolve_for_client(&commission_id, &String::from_str(&env, "Refunded"));
    let record = client.get_dispute(&commission_id);
    assert_eq!(record.status, DisputeStatus::ResolvedForClient);
}

#[test]
fn test_resolve_for_client_not_found_fails() {
    let (env, admin, escrow, config, _token) = create_test_env();
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let result = client.try_resolve_for_client(&commission_id, &String::from_str(&env, "note"));
    assert_eq!(result.unwrap_err().unwrap(), DisputeError::NotFound);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let result = client.try_resolve_for_client(&commission_id, &String::from_str(&env, "note"));
    assert!(result.is_err());
}

#[test]
#[ignore = "requires a live escrow contract"]
fn test_resolve_for_client_wrong_status_fails() {
    let (env, admin, escrow, config, _token) = create_test_env();
    let initiator = Address::generate(&env);
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    client.open_dispute(&commission_id, &initiator);
    client.resolve_for_client(&commission_id, &String::from_str(&env, "first"));
    let result = client.try_resolve_for_client(&commission_id, &String::from_str(&env, "second"));
    assert_eq!(result.unwrap_err().unwrap(), DisputeError::InvalidStatus);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let _ = client.open_dispute(&commission_id, &initiator);
    let _ = client.resolve_for_client(&commission_id, &String::from_str(&env, "first"));
    let result = client.try_resolve_for_client(&commission_id, &String::from_str(&env, "second"));
    assert!(result.is_err());
}

// ========== resolve_for_artist ==========

#[test]
#[ignore = "requires a live escrow contract"]
fn test_resolve_for_artist_succeeds() {
    let (env, admin, escrow, config, _token) = create_test_env();
    let initiator = Address::generate(&env);
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    client.open_dispute(&commission_id, &initiator);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let _ = client.open_dispute(&commission_id, &initiator);
    client.resolve_for_artist(&commission_id, &String::from_str(&env, "Paid"));
    let record = client.get_dispute(&commission_id);
    assert_eq!(record.status, DisputeStatus::ResolvedForArtist);
}

#[test]
fn test_resolve_for_artist_not_found_fails() {
    let (env, admin, escrow, config, _token) = create_test_env();
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let result = client.try_resolve_for_artist(&commission_id, &String::from_str(&env, "note"));
    assert_eq!(result.unwrap_err().unwrap(), DisputeError::NotFound);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let result = client.try_resolve_for_artist(&commission_id, &String::from_str(&env, "note"));
    assert!(result.is_err());
}

#[test]
#[ignore = "requires a live escrow contract"]
fn test_resolve_for_artist_wrong_status_fails() {
    let (env, admin, escrow, config, _token) = create_test_env();
    let initiator = Address::generate(&env);
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    client.open_dispute(&commission_id, &initiator);
    client.resolve_for_artist(&commission_id, &String::from_str(&env, "first"));
    let result = client.try_resolve_for_artist(&commission_id, &String::from_str(&env, "second"));
    assert_eq!(result.unwrap_err().unwrap(), DisputeError::InvalidStatus);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let _ = client.open_dispute(&commission_id, &initiator);
    let _ = client.resolve_for_artist(&commission_id, &String::from_str(&env, "first"));
    let result = client.try_resolve_for_artist(&commission_id, &String::from_str(&env, "second"));
    assert!(result.is_err());
}

// ========== partial_resolve ==========

#[test]
#[ignore = "requires a live escrow contract"]
fn test_partial_resolve_4000_bps() {
    let (env, admin, escrow, config, _token) = create_test_env();
    let initiator = Address::generate(&env);
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    client.open_dispute(&commission_id, &initiator);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let _ = client.open_dispute(&commission_id, &initiator);
    client.partial_resolve(&commission_id, &4000u32, &String::from_str(&env, "40pct client"));
    let record = client.get_dispute(&commission_id);
    assert_eq!(record.status, DisputeStatus::PartiallyResolved);
}

#[test]
fn test_partial_resolve_invalid_bps_fails() {
    let (env, admin, escrow, config, _token) = create_test_env();
    let initiator = Address::generate(&env);
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    client.open_dispute(&commission_id, &initiator);
    let result = client.try_partial_resolve(&commission_id, &10001u32, &String::from_str(&env, "bad"));
    assert_eq!(result.unwrap_err().unwrap(), DisputeError::InvalidShareBps);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let _ = client.open_dispute(&commission_id, &initiator);
    let result = client.try_partial_resolve(&commission_id, &10001u32, &String::from_str(&env, "bad"));
    assert!(result.is_err());
}

#[test]
#[ignore = "requires a live escrow contract"]
fn test_partial_resolve_valid_bps_boundary() {
    let (env, admin, escrow, config, _token) = create_test_env();
    let initiator = Address::generate(&env);
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    client.open_dispute(&commission_id, &initiator);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let _ = client.open_dispute(&commission_id, &initiator);
    client.partial_resolve(&commission_id, &10000u32, &String::from_str(&env, "all_client"));
}

#[test]
fn test_partial_resolve_not_found_fails() {
    let (env, admin, escrow, config, _token) = create_test_env();
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let result = client.try_partial_resolve(&commission_id, &5000u32, &String::from_str(&env, "note"));
    assert_eq!(result.unwrap_err().unwrap(), DisputeError::NotFound);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let result = client.try_partial_resolve(&commission_id, &5000u32, &String::from_str(&env, "note"));
    assert!(result.is_err());
}

#[test]
#[ignore = "requires a live escrow contract"]
fn test_partial_resolve_wrong_status_fails() {
    let (env, admin, escrow, config, _token) = create_test_env();
    let initiator = Address::generate(&env);
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    client.open_dispute(&commission_id, &initiator);
    client.resolve_for_client(&commission_id, &String::from_str(&env, "done"));
    let result = client.try_partial_resolve(&commission_id, &5000u32, &String::from_str(&env, "note"));
    assert_eq!(result.unwrap_err().unwrap(), DisputeError::InvalidStatus);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let _ = client.open_dispute(&commission_id, &initiator);
    let _ = client.resolve_for_client(&commission_id, &String::from_str(&env, "done"));
    let result = client.try_partial_resolve(&commission_id, &5000u32, &String::from_str(&env, "note"));
    assert!(result.is_err());
}

// ========== auto_resolve ==========

#[test]
fn test_auto_resolve_before_timeout_fails() {
    let (env, admin, escrow, config, _token) = create_test_env();
    let initiator = Address::generate(&env);
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    client.open_dispute(&commission_id, &initiator);
    let result = client.try_auto_resolve(&commission_id);
    assert_eq!(result.unwrap_err().unwrap(), DisputeError::AutoResolveNotDue);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let _ = client.open_dispute(&commission_id, &initiator);
    let result = client.try_auto_resolve(&commission_id);
    assert!(result.is_err());
}

#[test]
fn test_auto_resolve_at_timeout_succeeds() {
    let (env, admin, escrow, config, _token) = create_test_env();
    let initiator = Address::generate(&env);
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    client.open_dispute(&commission_id, &initiator);
    env.ledger().with_mut(|l| l.sequence_number = 101);
    client.auto_resolve(&commission_id);
    let record = client.get_dispute(&commission_id);
    assert_eq!(record.status, DisputeStatus::AutoResolved);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let _ = client.open_dispute(&commission_id, &initiator);
    env.ledger().with_mut(|l| l.sequence_number = 101);
    let _ = client.try_auto_resolve(&commission_id);
}

#[test]
fn test_auto_resolve_after_timeout_succeeds() {
    let (env, admin, escrow, config, _token) = create_test_env();
    let initiator = Address::generate(&env);
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    client.open_dispute(&commission_id, &initiator);
    env.ledger().with_mut(|l| l.sequence_number = 200);
    client.auto_resolve(&commission_id);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let _ = client.open_dispute(&commission_id, &initiator);
    env.ledger().with_mut(|l| l.sequence_number = 200);
    let _ = client.try_auto_resolve(&commission_id);
}

#[test]
fn test_auto_resolve_not_found_fails() {
    let (env, admin, escrow, config, _token) = create_test_env();
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let result = client.try_auto_resolve(&commission_id);
    assert_eq!(result.unwrap_err().unwrap(), DisputeError::NotFound);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let result = client.try_auto_resolve(&commission_id);
    assert!(result.is_err());
}

#[test]
#[ignore = "requires a live escrow contract"]
fn test_auto_resolve_wrong_status_fails() {
    let (env, admin, escrow, config, _token) = create_test_env();
    let initiator = Address::generate(&env);
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    client.open_dispute(&commission_id, &initiator);
    env.ledger().with_mut(|l| l.sequence_number = 101);
    client.auto_resolve(&commission_id);
    let result = client.try_auto_resolve(&commission_id);
    assert_eq!(result.unwrap_err().unwrap(), DisputeError::InvalidStatus);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let _ = client.open_dispute(&commission_id, &initiator);
    env.ledger().with_mut(|l| l.sequence_number = 101);
    let _ = client.auto_resolve(&commission_id);
    let result = client.try_auto_resolve(&commission_id);
    assert!(result.is_err());
}

// ========== get_dispute ==========

#[test]
fn test_get_dispute_succeeds() {
    let (env, admin, escrow, config, _token) = create_test_env();
    let initiator = Address::generate(&env);
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    client.open_dispute(&commission_id, &initiator);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let _ = client.open_dispute(&commission_id, &initiator);
    let record = client.get_dispute(&commission_id);
    assert_eq!(record.commission_id, commission_id);
    assert_eq!(record.status, DisputeStatus::Open);
}

#[test]
fn test_get_dispute_not_found_fails() {
    let (env, admin, escrow, config, _token) = create_test_env();
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let result = client.try_get_dispute(&commission_id);
    assert_eq!(result.unwrap_err().unwrap(), DisputeError::NotFound);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let result = client.try_get_dispute(&commission_id);
    assert!(result.is_err());
}

// ========== error codes ==========

#[test]
fn test_error_codes() {
    assert_eq!(DisputeError::AlreadyInitialized as u32, 1);
    assert_eq!(DisputeError::NotInitialized as u32, 2);
    assert_eq!(DisputeError::Unauthorized as u32, 3);
    assert_eq!(DisputeError::NotFound as u32, 4);
    assert_eq!(DisputeError::InvalidStatus as u32, 5);
    assert_eq!(DisputeError::AlreadyResolved as u32, 6);
    assert_eq!(DisputeError::AutoResolveNotDue as u32, 7);
    assert_eq!(DisputeError::InvalidShareBps as u32, 8);
}

// ========== dispute status values ==========

#[test]
fn test_dispute_status_values() {
    assert_eq!(DisputeStatus::Open as u32, 0);
    assert_eq!(DisputeStatus::ResolvedForClient as u32, 1);
    assert_eq!(DisputeStatus::ResolvedForArtist as u32, 2);
    assert_eq!(DisputeStatus::PartiallyResolved as u32, 3);
    assert_eq!(DisputeStatus::AutoResolved as u32, 4);
}

// ========== events ==========

#[test]
fn test_open_dispute_emits_event() {
    let (env, admin, escrow, config, _token) = create_test_env();
    let initiator = Address::generate(&env);
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    client.open_dispute(&commission_id, &initiator);
    let events = env.events().all();
    assert!(!events.is_empty());
}

#[test]
#[ignore = "requires a live escrow contract"]
fn test_resolve_for_client_emits_event() {
    let (env, admin, escrow, config, _token) = create_test_env();
    let initiator = Address::generate(&env);
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    client.open_dispute(&commission_id, &initiator);
    client.resolve_for_client(&commission_id, &String::from_str(&env, "Refunded"));
    let events = env.events().all();
    assert!(events.len() >= 2);
}

#[test]
#[ignore = "requires a live escrow contract"]
fn test_resolve_for_artist_emits_event() {
    let (env, admin, escrow, config, _token) = create_test_env();
    let initiator = Address::generate(&env);
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    client.open_dispute(&commission_id, &initiator);
    client.resolve_for_artist(&commission_id, &String::from_str(&env, "Paid"));
    let events = env.events().all();
    assert!(events.len() >= 2);
}

#[test]
#[ignore = "requires a live escrow contract"]
fn test_partial_resolve_emits_event() {
    let (env, admin, escrow, config, _token) = create_test_env();
    let initiator = Address::generate(&env);
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    client.open_dispute(&commission_id, &initiator);
    client.partial_resolve(&commission_id, &4000u32, &String::from_str(&env, "split"));
    let events = env.events().all();
    assert!(events.len() >= 2);
}

#[test]
#[ignore = "requires a live escrow contract"]
fn test_auto_resolve_emits_event() {
    let (env, admin, escrow, config, _token) = create_test_env();
    let initiator = Address::generate(&env);
    env.mock_all_auths();
    let arbiter = env.register_contract(None, DisputeArbiter);
    let client = DisputeArbiterClient::new(&env, &arbiter);
    client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    client.open_dispute(&commission_id, &initiator);
    env.ledger().with_mut(|l| l.sequence_number = 101);
    client.auto_resolve(&commission_id);
    let _ = client.initialize(&admin, &escrow, &config, &100u32);
    let commission_id = Bytes::from_array(&env, &[1u8, 2, 3]);
    let _ = client.open_dispute(&commission_id, &initiator);
    env.ledger().with_mut(|l| l.sequence_number = 101);
    let _ = client.auto_resolve(&commission_id);
    let events = env.events().all();
    assert!(events.len() >= 2);
}
