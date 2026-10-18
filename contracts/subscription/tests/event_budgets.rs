//! Event-count regression guard for the subscription contract (closes #994).
//!
//! Mirrors the pattern from `contracts/verification/tests/event_budgets.rs`.
//! Each operation must emit exactly the documented number of events; an extra
//! `publish` call added anywhere will make this test fail under `cargo test`
//! before it reaches a fee regression on-chain.
//!
//! Run with:
//!   cargo test -p subscription -- event_budgets

extern crate std;

use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _, Ledger},
    token, vec, Address, Env, String, Symbol, Vec as SorobanVec,
};

use subscription::{SubscriptionContract, SubscriptionContractClient};

const GRACE: u32 = 100;
const HISTORY_LIMIT: u32 = 50;
const PERIOD: u32 = 1_000;
const TIER_ID: u32 = 1;
const PRICE: i128 = 100;

/// How many contract events one call published.
fn events_emitted_by<F: FnOnce()>(env: &Env, call: F) -> u32 {
    let before = env.events().all().len();
    call();
    (env.events().all().len() - before) as u32
}

struct Fixture<'a> {
    env: Env,
    client: SubscriptionContractClient<'a>,
    admin: Address,
    token: Address,
}

fn setup<'a>() -> Fixture<'a> {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin).address();
    // Mint to every address we'll use so charges never fail.
    token::StellarAssetClient::new(&env, &token).mint(&admin, &1_000_000_000);

    let contract_id = env.register_contract(None, SubscriptionContract);
    let client = SubscriptionContractClient::new(&env, &contract_id);
    client.initialize(&admin, &token, &GRACE, &HISTORY_LIMIT).unwrap();

    // Create a default tier.
    let benefits: SorobanVec<Symbol> = SorobanVec::new(&env);
    client
        .create_tier(
            &TIER_ID,
            &String::from_str(&env, "Basic"),
            &PRICE,
            &PERIOD,
            &benefits,
        )
        .unwrap();

    Fixture {
        env,
        client,
        admin,
        token,
    }
}

fn new_subscriber(f: &Fixture) -> Address {
    let sub = Address::generate(&f.env);
    // Pre-fund the subscriber's credit so `subscribe` does not fail.
    token::StellarAssetClient::new(&f.env, &f.token).mint(&sub, &1_000_000);
    f.client.deposit(&sub, &1_000_000).unwrap();
    sub
}

/// `initialize` publishes exactly 1 event (`init`).
#[test]
fn initialize_emits_one_event() {
    // Use a fresh env so we're not counting events from `setup`.
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin).address();

    let contract_id = env.register_contract(None, SubscriptionContract);
    let client = SubscriptionContractClient::new(&env, &contract_id);

    assert_eq!(
        events_emitted_by(&env, || {
            client
                .initialize(&admin, &token, &GRACE, &HISTORY_LIMIT)
                .unwrap();
        }),
        1
    );
}

/// `create_tier` publishes exactly 1 event (`tier_new`).
#[test]
fn create_tier_emits_one_event() {
    let f = setup();
    let benefits: SorobanVec<Symbol> = SorobanVec::new(&f.env);
    assert_eq!(
        events_emitted_by(&f.env, || {
            f.client
                .create_tier(
                    &2u32,
                    &String::from_str(&f.env, "Pro"),
                    &(PRICE * 2),
                    &PERIOD,
                    &benefits,
                )
                .unwrap();
        }),
        1
    );
}

/// `subscribe` publishes exactly 1 event (`subbed`).
#[test]
fn subscribe_emits_one_event() {
    let f = setup();
    let sub = new_subscriber(&f);
    assert_eq!(
        events_emitted_by(&f.env, || {
            f.client.subscribe(&sub, &TIER_ID, &false).unwrap();
        }),
        1
    );
}

/// `renew` publishes exactly 1 event (`renewed`).
#[test]
fn renew_emits_one_event() {
    let f = setup();
    let sub = new_subscriber(&f);
    f.client.subscribe(&sub, &TIER_ID, &false).unwrap();
    // Advance past the current period to trigger a renewal.
    f.env.ledger().with_mut(|l| l.sequence_number += PERIOD + 1);

    assert_eq!(
        events_emitted_by(&f.env, || {
            f.client.renew(&sub).unwrap();
        }),
        1
    );
}

/// `cancel` publishes exactly 1 event (`cancelled`).
#[test]
fn cancel_emits_one_event() {
    let f = setup();
    let sub = new_subscriber(&f);
    f.client.subscribe(&sub, &TIER_ID, &false).unwrap();
    assert_eq!(
        events_emitted_by(&f.env, || {
            f.client.cancel(&sub).unwrap();
        }),
        1
    );
}

/// `lapse` publishes exactly 1 event (`lapsed`).
#[test]
fn lapse_emits_one_event() {
    let f = setup();
    let sub = new_subscriber(&f);
    f.client.subscribe(&sub, &TIER_ID, &false).unwrap();
    // Advance past the period end (and grace) so the subscription can be lapsed.
    f.env
        .ledger()
        .with_mut(|l| l.sequence_number += PERIOD + GRACE + 1);
    assert_eq!(
        events_emitted_by(&f.env, || {
            f.client.lapse(&sub).unwrap();
        }),
        1
    );
}
