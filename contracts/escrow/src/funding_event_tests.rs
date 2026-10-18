//! Tests for the escrow funding notification (#978).
//!
//! These assert the *additive* `escrow`/`funded` topic and the atomic-commit
//! finalization event, and — importantly — that the pre-existing
//! `escrow`/`created` payload is unchanged (it is indexed downstream).

extern crate std;
use soroban_sdk::{
    contract, contractimpl, symbol_short, testutils::Address as _, testutils::Events, token,
    Address, Bytes, Env, Symbol, TryFromVal, Val,
};

use crate::cross_contract::AtomicCommitState;
use crate::{EscrowContract, EscrowContractClient};

const FEE_BPS: u32 = 500;
const AMOUNT: i128 = 10_000;

/// Minimal platform-config stand-in so `create_escrow` can resolve fee/token.
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

struct Fixture<'a> {
    env: Env,
    escrow: EscrowContractClient<'a>,
    config: Address,
    usdc: Address,
    client: Address,
    artist: Address,
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

    let escrow_id = env.register_contract(None, EscrowContract);
    let escrow = EscrowContractClient::new(&env, &escrow_id);

    Fixture {
        env,
        escrow,
        config,
        usdc,
        client,
        artist,
        commission_id: Bytes::from_slice(&env, b"comm-001"),
    }
}

/// Return the data of the first two-topic event matching `t0`/`t1`.
fn two_topic_data(f: &Fixture, t0: &str, t1: &str) -> Option<Val> {
    let want0 = Symbol::new(&f.env, t0);
    let want1 = Symbol::new(&f.env, t1);
    for (_contract, topics, data) in f.env.events().all().iter() {
        if topics.len() != 2 {
            continue;
        }
        let a: Symbol = Symbol::try_from_val(&f.env, &topics.get(0).unwrap()).unwrap();
        let b: Symbol = Symbol::try_from_val(&f.env, &topics.get(1).unwrap()).unwrap();
        if a == want0 && b == want1 {
            return Some(data);
        }
    }
    None
}

#[test]
fn create_escrow_emits_funding_event_with_expected_values() {
    let f = setup();
    f.escrow
        .create_escrow(&f.commission_id, &f.client, &f.artist, &AMOUNT, &f.config);

    let data = two_topic_data(&f, "escrow", "funded")
        .expect("expected an (escrow, funded) event");
    let (commission_id, funder, amount, asset, ledger): (Bytes, Address, i128, Address, u32) =
        TryFromVal::try_from_val(&f.env, &data).unwrap();

    assert_eq!(commission_id, f.commission_id);
    assert_eq!(funder, f.client);
    assert_eq!(amount, AMOUNT);
    assert_eq!(asset, f.usdc);
    assert_eq!(ledger, f.env.ledger().sequence());
}

#[test]
fn created_payload_is_unchanged() {
    let f = setup();
    f.escrow
        .create_escrow(&f.commission_id, &f.client, &f.artist, &AMOUNT, &f.config);

    let data =
        two_topic_data(&f, "escrow", "created").expect("expected an (escrow, created) event");
    let (commission_id, amount): (Bytes, i128) = TryFromVal::try_from_val(&f.env, &data).unwrap();
    assert_eq!(commission_id, f.commission_id);
    assert_eq!(amount, AMOUNT);
}

#[test]
fn finalize_atomic_commit_publishes_settled_event() {
    let f = setup();
    f.escrow
        .create_escrow(&f.commission_id, &f.client, &f.artist, &AMOUNT, &f.config);

    let marker = f.escrow.begin_atomic_commit(&f.commission_id);
    assert_eq!(marker.state, AtomicCommitState::InProgress);

    f.escrow.confirm_atomic_step(&f.commission_id, &f.client);
    f.escrow.confirm_atomic_step(&f.commission_id, &f.artist);

    let final_marker = f.escrow.finalize_atomic_commit(&f.commission_id);
    assert_eq!(final_marker.state, AtomicCommitState::Settled);

    let data = two_topic_data(&f, "escrow", "settled")
        .expect("expected an (escrow, settled) event on finalization");
    let (commission_id, ledger): (Bytes, u32) = TryFromVal::try_from_val(&f.env, &data).unwrap();
    assert_eq!(commission_id, f.commission_id);
    assert_eq!(ledger, f.env.ledger().sequence());
}
