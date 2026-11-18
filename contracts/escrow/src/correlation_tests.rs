//! Tests for inter-contract event correlation (#661).

extern crate std;
use soroban_sdk::{
    contract, contractimpl, symbol_short, testutils::Address as _, testutils::Events, token,
    Address, Bytes, BytesN, Env, Symbol, TryFromVal,
};

use crate::storage::CommissionStatus;
use crate::{EscrowContract, EscrowContractClient};

/// Minimal platform-config stand-in so `create_escrow` can resolve fee/token.
#[contract]
pub struct MockConfig;

#[contractimpl]
impl MockConfig {
    pub fn init(env: Env, admin: Address, usdc: Address, pw: Address) {
        env.storage().instance().set(&0u32, &admin);
        env.storage().instance().set(&1u32, &usdc);
        env.storage().instance().set(&2u32, &pw);
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
        500
    }
}

// ── shared correlation primitives ──────────────────────────────────────────

#[test]
fn derive_is_deterministic() {
    let env = Env::default();
    let scope = shared::correlation::scope(&env, "escrow");
    let part = Bytes::from_slice(&env, b"comm-001");
    let parts = [&part];
    let a = shared::correlation::CorrelationId::derive(&env, &scope, &parts);
    let b = shared::correlation::CorrelationId::derive(&env, &scope, &parts);
    assert_eq!(a, b);
}

#[test]
fn derive_changes_with_scope_or_parts() {
    let env = Env::default();
    let escrow = shared::correlation::scope(&env, "escrow");
    let agr = shared::correlation::scope(&env, "agr");
    let key1 = Bytes::from_slice(&env, b"comm-001");
    let key2 = Bytes::from_slice(&env, b"comm-002");

    let escrow_key1 = shared::correlation::CorrelationId::derive(&env, &escrow, &[&key1]);
    let agr_key1 = shared::correlation::CorrelationId::derive(&env, &agr, &[&key1]);
    let escrow_key2 = shared::correlation::CorrelationId::derive(&env, &escrow, &[&key2]);

    assert_ne!(escrow_key1, agr_key1);
    assert_ne!(escrow_key1, escrow_key2);
}

#[test]
fn link_encodes_causality() {
    let env = Env::default();
    let scope = shared::correlation::scope(&env, "dispute");
    let parent = shared::correlation::CorrelationId::derive(
        &env,
        &scope,
        &[&Bytes::from_slice(&env, b"comm-001")],
    );
    let child = shared::correlation::CorrelationId::derive(
        &env,
        &scope,
        &[
            &Bytes::from_slice(&env, b"comm-001"),
            &Bytes::from_slice(&env, b"appeal"),
        ],
    );

    let chained = shared::correlation::CorrelationId::link(&env, &parent, &child);
    assert_ne!(chained, parent);
    assert_ne!(chained, child);

    // Linking is deterministic too, so replayed event streams agree.
    let chained_again = shared::correlation::CorrelationId::link(&env, &parent, &child);
    assert_eq!(chained, chained_again);
}

#[test]
fn to_bytes_roundtrips() {
    let env = Env::default();
    let scope = shared::correlation::scope(&env, "escrow");
    let id = shared::correlation::CorrelationId::derive(
        &env,
        &scope,
        &[&Bytes::from_slice(&env, b"comm-001")],
    );
    let bytes = id.to_bytes(&env);
    assert_eq!(bytes.len(), 32);
    let mut arr = [0u8; 32];
    bytes.copy_into_slice(&mut arr);
    let back: BytesN<32> = BytesN::from_array(&env, &arr);
    assert_eq!(back, id.id);
}

// ── escrow integration of correlation events ───────────────────────────────

struct Fixture {
    env: Env,
    escrow: EscrowContractClient<'static>,
    config: Address,
    client: Address,
    artist: Address,
    commission_id: Bytes,
}

fn setup() -> Fixture {
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
    let commission_id = Bytes::from_slice(&env, b"comm-001");

    Fixture {
        env,
        escrow,
        config,
        client,
        artist,
        commission_id,
    }
}

fn find_corr_event(f: &Fixture) -> bool {
    let events = f.env.events().all();
    for (_contract, topics, _data) in events.iter() {
        if topics.len() != 3 {
            continue;
        }
        let d: Symbol = Symbol::try_from_val(&f.env, &topics.get(0).unwrap()).unwrap();
        if d != symbol_short!("escrow") {
            continue;
        }
        let a: Symbol = Symbol::try_from_val(&f.env, &topics.get(1).unwrap()).unwrap();
        if a != symbol_short!("created") {
            continue;
        }
        let c: Symbol = Symbol::try_from_val(&f.env, &topics.get(2).unwrap()).unwrap();
        if c == Symbol::new(&f.env, "corr") {
            return true;
        }
    }
    false
}

#[test]
fn create_escrow_emits_correlated_event() {
    let f = setup();
    let amount = 10_000i128;
    f.escrow.create_escrow(&f.commission_id, &f.client, &f.artist, &amount, &f.config);
    assert!(find_corr_event(&f), "expected a (escrow, created, corr) correlation event");
}

#[test]
fn primary_event_schema_is_preserved() {
    let f = setup();
    let amount = 10_000i128;
    f.escrow.create_escrow(&f.commission_id, &f.client, &f.artist, &amount, &f.config);
    f.escrow.release_payment(&f.commission_id, &f.config);

    let events = f.env.events().all();
    let mut primary = false;
    let mut corr = false;
    for (_contract, topics, _data) in events.iter() {
        if topics.len() == 2 {
            let d: Symbol = Symbol::try_from_val(&f.env, &topics.get(0).unwrap()).unwrap();
            let a: Symbol = Symbol::try_from_val(&f.env, &topics.get(1).unwrap()).unwrap();
            if d == symbol_short!("escrow")
                && (a == symbol_short!("created") || a == symbol_short!("released"))
            {
                primary = true;
            }
        } else if topics.len() == 3 {
            let d: Symbol = Symbol::try_from_val(&f.env, &topics.get(0).unwrap()).unwrap();
            if d != symbol_short!("escrow") {
                continue;
            }
            let a: Symbol = Symbol::try_from_val(&f.env, &topics.get(1).unwrap()).unwrap();
            let c: Symbol = Symbol::try_from_val(&f.env, &topics.get(2).unwrap()).unwrap();
            if a == symbol_short!("created") && c == Symbol::new(&f.env, "corr") {
                corr = true;
            }
        }
    }
    assert!(primary, "primary 2-topic events must be unchanged");
    assert!(corr, "correlation events must also be present");
}

#[test]
fn escrow_status_survives_correlated_flow() {
    let f = setup();
    let amount = 10_000i128;
    f.escrow.create_escrow(&f.commission_id, &f.client, &f.artist, &amount, &f.config);
    let record = f.escrow.get_escrow(&f.commission_id);
    assert_eq!(record.status, CommissionStatus::Locked);
}

#[test]
fn shared_toolkit_is_reachable_from_escrow() {
    let f = setup();
    let scope = shared::correlation::scope(&f.env, "escrow");
    let _id = shared::correlation::CorrelationId::derive(&f.env, &scope, &[&f.commission_id]);
}