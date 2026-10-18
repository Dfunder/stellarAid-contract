//! Event-count regression guard for the competitions contract (closes #994).
//!
//! Mirrors the pattern from `contracts/verification/tests/event_budgets.rs`.
//! Each operation must emit exactly the documented number of events; a second
//! `publish` call added anywhere in the call tree will make this test fail
//! before it registers as a fee regression on-chain.
//!
//! How events are counted: `env.events().all()` returns the test host's
//! cumulative event log, so a raw length grows across the whole test. Only
//! the *difference* around each call is an event count for that specific call.
//! See `events_emitted_by` below.
//!
//! Run with:
//!   cargo test -p competitions -- event_budgets

extern crate std;

use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger},
    token, vec, Address, Bytes, Env, String,
};

use competitions::types::CompetitionRules;
use competitions::{Competitions, CompetitionsClient};

const SUBMISSION_LEDGERS: u32 = 100;
const VOTING_LEDGERS: u32 = 100;
const PRIZE_POOL: i128 = 10_000;
const MIN_REPUTATION: u32 = 0; // zero so any address can vote in tests
const HISTORY_LIMIT: u32 = 50;

/// How many contract events one call published.
///
/// `env.events().all()` hands back the test host's whole event log, not just
/// the most recent invocation, so a raw length grows with every call on the
/// same `Env`. Only the difference taken around the call under test is
/// meaningful.
fn events_emitted_by<F: FnOnce()>(env: &Env, call: F) -> u32 {
    let before = env.events().all().len();
    call();
    (env.events().all().len() - before) as u32
}

fn comp_id(env: &Env, tag: &str) -> Bytes {
    Bytes::from_slice(env, tag.as_bytes())
}

fn rules(env: &Env) -> CompetitionRules {
    CompetitionRules {
        submission_ledgers: SUBMISSION_LEDGERS,
        voting_ledgers: VOTING_LEDGERS,
        max_submissions: 10_000,
        min_reputation: MIN_REPUTATION,
        prize_split_bps: vec![env, 6_000, 4_000],
    }
}

struct Fixture<'a> {
    env: Env,
    client: CompetitionsClient<'a>,
    organizer: Address,
    token: Address,
}

fn setup<'a>() -> Fixture<'a> {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let organizer = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin).address();
    token::StellarAssetClient::new(&env, &token).mint(&organizer, &1_000_000_000);

    let contract_id = env.register_contract(None, Competitions);
    let client = CompetitionsClient::new(&env, &contract_id);
    client.initialize(&admin, &HISTORY_LIMIT);

    Fixture {
        env,
        client,
        organizer,
        token,
    }
}

/// `create_competition` publishes exactly 1 event (`comp_new`).
#[test]
fn create_competition_emits_one_event() {
    let f = setup();
    let id = comp_id(&f.env, "budget-create");
    assert_eq!(
        events_emitted_by(&f.env, || {
            f.client.create_competition(
                &id,
                &f.organizer,
                &f.token,
                &String::from_str(&f.env, "Test"),
                &PRIZE_POOL,
                &rules(&f.env),
            )
            .unwrap();
        }),
        1
    );
}

/// `submit` publishes exactly 1 event (`submit`).
#[test]
fn submit_emits_one_event() {
    let f = setup();
    let id = comp_id(&f.env, "budget-submit");
    f.client
        .create_competition(
            &id,
            &f.organizer,
            &f.token,
            &String::from_str(&f.env, "Test"),
            &PRIZE_POOL,
            &rules(&f.env),
        )
        .unwrap();
    let entrant = Address::generate(&f.env);

    assert_eq!(
        events_emitted_by(&f.env, || {
            f.client
                .submit(&id, &entrant, &String::from_str(&f.env, "ipfs://entry"))
                .unwrap();
        }),
        1
    );
}

/// `vote` publishes exactly 1 event (`vote`).
#[test]
fn vote_emits_one_event() {
    let f = setup();
    let id = comp_id(&f.env, "budget-vote");
    f.client
        .create_competition(
            &id,
            &f.organizer,
            &f.token,
            &String::from_str(&f.env, "Test"),
            &PRIZE_POOL,
            &rules(&f.env),
        )
        .unwrap();
    let entrant = Address::generate(&f.env);
    let voter = Address::generate(&f.env);
    f.client
        .submit(&id, &entrant, &String::from_str(&f.env, "ipfs://entry"))
        .unwrap();
    f.client.set_reputation(&voter, &30).unwrap();

    // Advance past submission window into voting window.
    f.env
        .ledger()
        .with_mut(|l| l.sequence_number += SUBMISSION_LEDGERS + 1);

    assert_eq!(
        events_emitted_by(&f.env, || {
            f.client.vote(&id, &voter, &entrant).unwrap();
        }),
        1
    );
}

/// `finalize` publishes exactly 1 event (`final`).
#[test]
fn finalize_emits_one_event() {
    let f = setup();
    let id = comp_id(&f.env, "budget-finalize");
    f.client
        .create_competition(
            &id,
            &f.organizer,
            &f.token,
            &String::from_str(&f.env, "Test"),
            &PRIZE_POOL,
            &rules(&f.env),
        )
        .unwrap();
    let entrant = Address::generate(&f.env);
    let voter = Address::generate(&f.env);
    f.client
        .submit(&id, &entrant, &String::from_str(&f.env, "ipfs://entry"))
        .unwrap();
    f.client.set_reputation(&voter, &30).unwrap();
    f.env
        .ledger()
        .with_mut(|l| l.sequence_number += SUBMISSION_LEDGERS + 1);
    f.client.vote(&id, &voter, &entrant).unwrap();
    // Advance past the voting window.
    f.env
        .ledger()
        .with_mut(|l| l.sequence_number += VOTING_LEDGERS + 1);

    assert_eq!(
        events_emitted_by(&f.env, || {
            f.client.finalize(&id).unwrap();
        }),
        1
    );
}

/// `distribute_prizes` publishes exactly 1 event (`prizes`).
#[test]
fn distribute_prizes_emits_one_event() {
    let f = setup();
    let id = comp_id(&f.env, "budget-distribute");
    f.client
        .create_competition(
            &id,
            &f.organizer,
            &f.token,
            &String::from_str(&f.env, "Test"),
            &PRIZE_POOL,
            &rules(&f.env),
        )
        .unwrap();
    let entrant = Address::generate(&f.env);
    let voter = Address::generate(&f.env);
    f.client
        .submit(&id, &entrant, &String::from_str(&f.env, "ipfs://entry"))
        .unwrap();
    f.client.set_reputation(&voter, &30).unwrap();
    f.env
        .ledger()
        .with_mut(|l| l.sequence_number += SUBMISSION_LEDGERS + 1);
    f.client.vote(&id, &voter, &entrant).unwrap();
    f.env
        .ledger()
        .with_mut(|l| l.sequence_number += VOTING_LEDGERS + 1);
    f.client.finalize(&id).unwrap();

    assert_eq!(
        events_emitted_by(&f.env, || {
            f.client.distribute_prizes(&id).unwrap();
        }),
        1
    );
}
