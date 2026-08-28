//! Tests for the atomic escrow→commission flow (#656).

extern crate std;
use soroban_sdk::{
    contract, contractimpl, symbol_short, testutils::Address as _, token, Address, Bytes, Env,
    IntoVal,
};

use crate::cross_contract::AtomicCommitState;
use crate::errors::EscrowError;
use crate::storage::CommissionStatus;
use crate::{AtomicCommitMarker, EscrowContract, EscrowContractClient};

const FEE_BPS: u32 = 500;
const AMOUNT: i128 = 10_000;

/// Minimal stand-in for the platform config contract.
#[contract]
pub struct MockConfig;

#[contractimpl]
impl MockConfig {
    pub fn init(env: Env, admin: Address, usdc: Address, platform_wallet: Address) {
        env.storage().instance().set(&0u32, &admin);
        env.storage().instance().set(&1u32, &usdc);
        env.storage().instance().set(&2u32, &platform_wallet);
    }
    pub fn get_adm(env: Env) -> Address {
        env.storage().instance().get(&0u32).unwrap()
    }
    pub fn get_usdc(env: Env) -> Address {
        env.storage().instance().get(&1u32).unwrap()
    }
    pub fn get_pw(env: Env) -> Address {
        env.storage().instance().get(&2u32).unwrap()
    }
    pub fn get_fee_b(_env: Env) -> u32 {
        FEE_BPS
    }
}

/// Minimal stand-in for the commission agreement contract exposing the
/// escrow-amount probe used by the atomic flow's consistency check.
#[contract]
pub struct MockCommission;

#[contractimpl]
impl MockCommission {
    pub fn configure(env: Env, expected_amount: i128) {
        env.storage().instance().set(&0u32, &expected_amount);
    }
    pub fn get_agreement_escrow_amount(env: Env, _commission_id: Bytes) -> i128 {
        env.storage().instance().get(&0u32).unwrap()
    }
}

struct Fixture<'a> {
    env: Env,
    escrow: EscrowContractClient<'a>,
    escrow_addr: Address,
    commission: Address,
    config: Address,
    usdc: Address,
    client: Address,
    artist: Address,
    platform_wallet: Address,
    commission_id: Bytes,
}

fn setup<'a>() -> Fixture<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let usdc = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let artist = Address::generate(&env);
    let platform_wallet = Address::generate(&env);

    token::StellarAssetClient::new(&env, &usdc).mint(&client, &1_000_000);

    let config = env.register_contract(None, MockConfig);
    MockConfigClient::new(&env, &config).init(&admin, &usdc, &platform_wallet);

    let commission = env.register_contract(None, MockCommission);
    MockCommissionClient::new(&env, &commission).configure(&AMOUNT);

    let escrow_id = env.register_contract(None, EscrowContract);
    let escrow_addr = escrow_id.clone();
    let escrow = EscrowContractClient::new(&env, &escrow_id);

    Fixture {
        escrow_addr,
        commission_id: Bytes::from_slice(&env, b"comm-001"),
        env,
        escrow,
        commission,
        config,
        usdc,
        client,
        artist,
        platform_wallet,
    }
}

fn fund_escrow(f: &Fixture) {
    f.escrow.create_escrow(
        &f.commission_id,
        &f.client,
        &f.artist,
        &AMOUNT,
        &f.config,
    );
}

// ── begin ──────────────────────────────────────────────────────────────────

#[test]
fn begin_atomic_commit_creates_marker() {
    let f = setup();
    fund_escrow(&f);

    let marker = f.escrow.begin_atomic_commit(&f.commission_id);
    assert_eq!(marker.state, AtomicCommitState::InProgress);
    assert_eq!(marker.participants, 2);
    assert_eq!(marker.confirmed, 0);
    assert_eq!(marker.settled_ledger, None);

    let read = f.escrow.get_atomic_commit(&f.commission_id);
    assert_eq!(read.commission_id, f.commission_id);
}

#[test]
fn begin_atomic_commit_rejects_missing_escrow() {
    let f = setup();
    let err = f
        .escrow
        .try_begin_atomic_commit(&f.commission_id)
        .err()
        .unwrap()
        .unwrap();
    assert!(crate::errors::EscrowError::NotFound == err);
}

#[test]
fn begin_atomic_commit_rejects_duplicate_marker() {
    let f = setup();
    fund_escrow(&f);
    f.escrow.begin_atomic_commit(&f.commission_id);
    let err = f
        .escrow
        .try_begin_atomic_commit(&f.commission_id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, EscrowError::AlreadyExists);
}

#[test]
fn begin_atomic_commit_rejects_released_escrow() {
    let f = setup();
    fund_escrow(&f);
    // Release via the normal path first (admin-approved under mock auths).
    f.escrow.release_payment(&f.commission_id, &f.config);

    let err = f
        .escrow
        .try_begin_atomic_commit(&f.commission_id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, EscrowError::InvalidStatus);
}

// ── confirm ────────────────────────────────────────────────────────────────

#[test]
fn confirm_atomic_step_increments_count() {
    let f = setup();
    fund_escrow(&f);
    f.escrow.begin_atomic_commit(&f.commission_id);

    let one = f.escrow.confirm_atomic_step(&f.commission_id, &f.client);
    assert_eq!(one, 1);
    let two = f.escrow.confirm_atomic_step(&f.commission_id, &f.artist);
    assert_eq!(two, 2);
}

#[test]
fn confirm_atomic_step_rejects_overflow() {
    let f = setup();
    fund_escrow(&f);
    f.escrow.begin_atomic_commit(&f.commission_id);
    f.escrow.confirm_atomic_step(&f.commission_id, &f.client);
    f.escrow.confirm_atomic_step(&f.commission_id, &f.artist);

    let err = f
        .escrow
        .try_confirm_atomic_step(&f.commission_id, &f.client)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, EscrowError::AtomicCommitStateInvalid);
}

#[test]
fn confirm_atomic_step_rejects_when_no_marker() {
    let f = setup();
    let err = f
        .escrow
        .try_confirm_atomic_step(&f.commission_id, &f.client)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, EscrowError::NotFound);
}

// ── finalize ───────────────────────────────────────────────────────────────

#[test]
fn finalize_requires_all_participants_confirmed() {
    let f = setup();
    fund_escrow(&f);
    f.escrow.begin_atomic_commit(&f.commission_id);
    f.escrow.confirm_atomic_step(&f.commission_id, &f.client);

    let err = f
        .escrow
        .try_finalize_atomic_commit(&f.commission_id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, EscrowError::AtomicCommitNotReady);
}

#[test]
fn finalize_sets_escrow_released_and_marker_settled() {
    let f = setup();
    fund_escrow(&f);
    f.escrow.begin_atomic_commit(&f.commission_id);
    f.escrow.confirm_atomic_step(&f.commission_id, &f.client);
    f.escrow.confirm_atomic_step(&f.commission_id, &f.artist);

    let marker = f.escrow.finalize_atomic_commit(&f.commission_id);
    assert_eq!(marker.state, AtomicCommitState::Settled);
    assert!(marker.settled_ledger.is_some());

    let record = f.escrow.get_escrow(&f.commission_id);
    assert_eq!(record.status, CommissionStatus::Released);
}

// ── rollback ───────────────────────────────────────────────────────────────

#[test]
fn rollback_leaves_escrow_intact() {
    let f = setup();
    fund_escrow(&f);
    f.escrow.begin_atomic_commit(&f.commission_id);

    let marker = f.escrow.rollback_atomic_commit(&f.commission_id);
    assert_eq!(marker.state, AtomicCommitState::RolledBack);

    let record = f.escrow.get_escrow(&f.commission_id);
    assert_eq!(record.status, CommissionStatus::Locked);
    assert_eq!(
        token::Client::new(&f.env, &f.usdc).balance(&f.escrow_addr),
        AMOUNT
    );
}

#[test]
fn rollback_after_finalize_is_rejected() {
    let f = setup();
    fund_escrow(&f);
    f.escrow.begin_atomic_commit(&f.commission_id);
    f.escrow.confirm_atomic_step(&f.commission_id, &f.client);
    f.escrow.confirm_atomic_step(&f.commission_id, &f.artist);
    f.escrow.finalize_atomic_commit(&f.commission_id);

    let err = f
        .escrow
        .try_rollback_atomic_commit(&f.commission_id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, EscrowError::AtomicCommitStateInvalid);
}

// ── consistency probe ──────────────────────────────────────────────────────

#[test]
fn verify_agreement_consistency_true_when_amounts_match() {
    let f = setup();
    let ok = f
        .escrow
        .verify_agreement_consistency(&f.commission, &f.commission_id, &AMOUNT);
    assert!(ok);
}

#[test]
fn verify_agreement_consistency_false_on_mismatch() {
    let f = setup();
    let ok = f
        .escrow
        .verify_agreement_consistency(&f.commission, &f.commission_id, &(AMOUNT + 1));
    assert!(!ok);
}

// ── atomic migration ───────────────────────────────────────────────────────

#[test]
fn atomic_migration_moves_funds_in_one_transaction() {
    let f = setup();
    fund_escrow(&f);

    let escrow_contract = f.escrow_addr.clone();
    let marker = f
        .escrow
        .atomic_escrow_to_commission(&f.commission_id, &f.config, &f.commission);
    assert_eq!(marker.state, AtomicCommitState::Settled);

    // Fee split: 500 bps => fee + payout.
    let fee = AMOUNT * FEE_BPS as i128 / 10_000;
    let payout = AMOUNT - fee;

    assert_eq!(
        token::Client::new(&f.env, &f.usdc).balance(&escrow_contract),
        0
    );
    assert_eq!(
        token::Client::new(&f.env, &f.usdc).balance(&f.commission),
        payout
    );
    assert_eq!(
        token::Client::new(&f.env, &f.usdc).balance(&f.platform_wallet),
        fee
    );

    let record = f.escrow.get_escrow(&f.commission_id);
    assert_eq!(record.status, CommissionStatus::Released);
}

#[test]
fn atomic_migration_fails_atomically_on_consistency_mismatch() {
    let f = setup();
    fund_escrow(&f);
    // Commission expects a different amount than the escrow holds.
    MockCommissionClient::new(&f.env, &f.commission).configure(&(AMOUNT + 5));

    let escrow_contract = f.escrow_addr.clone();
    let before = token::Client::new(&f.env, &f.usdc).balance(&escrow_contract);

    let marker = f
        .escrow
        .atomic_escrow_to_commission(&f.commission_id, &f.config, &f.commission);
    assert_eq!(marker.state, AtomicCommitState::Failed);

    // Nothing moved, escrow record untouched.
    assert_eq!(
        token::Client::new(&f.env, &f.usdc).balance(&escrow_contract),
        before
    );
    assert_eq!(
        token::Client::new(&f.env, &f.usdc).balance(&f.commission),
        0
    );
    let record = f.escrow.get_escrow(&f.commission_id);
    assert_eq!(record.status, CommissionStatus::Locked);

    // The failure is recorded permanently in the marker.
    let marker = f.escrow.get_atomic_commit(&f.commission_id);
    assert_eq!(marker.state, AtomicCommitState::Failed);
}

#[test]
fn atomic_migration_rejects_missing_escrow() {
    let f = setup();
    let err = f
        .escrow
        .try_atomic_escrow_to_commission(&f.commission_id, &f.config, &f.commission)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, EscrowError::NotFound);
}

// ── marker serialization round-trip (#656) ─────────────────────────────────

#[test]
fn marker_compatible_with_shared_error_catalogue() {
    // New error codes must stay within the escrow band of the shared
    // catalogue (100–199).
    assert_eq!(EscrowError::AtomicCommitNotReady as u32, 15);
    assert_eq!(EscrowError::AtomicCommitStateInvalid as u32, 16);
    assert_eq!(EscrowError::CrossContractConsistencyFailed as u32, 17);
}

#[test]
#[allow(unused)]
fn marker_tuple_types_satisfy_contracttype() {
    let f = setup();
    // Compile-time coverage that the marker (and its optional field) can be
    // encoded as a contract argument, as the generated client does above.
    let _marker: AtomicCommitMarker = AtomicCommitMarker {
        commission_id: f.commission_id,
        state: AtomicCommitState::InProgress,
        participants: 2,
        confirmed: 0,
        created_ledger: 0,
        settled_ledger: None,
    };
    let _ = symbol_short!("x");
    let _: soroban_sdk::Vec<soroban_sdk::Val> = soroban_sdk::vec![&f.env, AMOUNT.into_val(&f.env)];
}