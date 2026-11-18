//! Event-count regression guard for the verification contract (#873).
//!
//! This is an integration test rather than another `#[test]` in `src/test.rs`
//! on purpose: it needs nothing private from the crate, and keeping it out of
//! the shared unit-test module means this file does not collide with the
//! pagination tests added for #876 in the same file.

extern crate std;

use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger},
    Address, Env, String,
};
use verification::{
    types::{BadgeType, QualityScore},
    Verification, VerificationClient,
};

const MIN_SCORE: u32 = 70;
const MIN_WORK_COUNT: u32 = 3;
const UPDATE_INTERVAL: u32 = 1000;
const HISTORY_LIMIT: u32 = 3;

struct Fixture<'a> {
    env: Env,
    client: VerificationClient<'a>,
    reviewer: Address,
    artist: Address,
}

fn setup<'a>() -> Fixture<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let reviewer = Address::generate(&env);
    let artist = Address::generate(&env);
    let contract_id = env.register_contract(None, Verification);
    let client = VerificationClient::new(&env, &contract_id);
    client.initialize(
        &admin,
        &MIN_SCORE,
        &MIN_WORK_COUNT,
        &UPDATE_INTERVAL,
        &HISTORY_LIMIT,
    );
    client.add_reviewer(&reviewer);
    Fixture {
        env,
        client,
        reviewer,
        artist,
    }
}

fn quality(mark: u32) -> QualityScore {
    QualityScore {
        originality: mark,
        technique: mark,
        consistency: mark,
        presentation: mark,
    }
}

fn uri(env: &Env) -> String {
    String::from_str(env, "ipfs://portfolio")
}

fn note(env: &Env) -> String {
    String::from_str(env, "reviewed manually")
}

/// How many contract events one call published.
///
/// `env.events().all()` hands back the test host's whole event log, not just the
/// most recent invocation, so a raw length grows with every call on the same
/// `Env` and says nothing about the call being measured. Only the difference
/// taken around it is an event count. `tests/framework/tests/pause_recovery.rs`
/// budgets its events the same way.
fn events_emitted_by<F: FnOnce()>(env: &Env, call: F) -> u32 {
    let before = env.events().all().len();
    call();
    (env.events().all().len() - before) as u32
}

/// Mirrors the `BUDGETS` table in `benches/verification_benchmark.rs`; keep the
/// two in sync. The benchmark enforces that budget, and this test makes a change
/// that breaks it fail under a plain `cargo test` with no benchmark run needed.
///
/// Each of these operations publishes exactly one event. A second `publish`
/// added for logging or indexing shows up here as a failing assertion before it
/// shows up as a fee regression on-chain.
#[test]
fn event_budgets_match_benchmark() {
    let f = setup();

    assert_eq!(
        events_emitted_by(&f.env, || {
            f.client.submit_portfolio(&f.artist, &uri(&f.env), &5);
        }),
        1
    );

    assert_eq!(
        events_emitted_by(&f.env, || {
            f.client.start_review(&f.reviewer, &f.artist);
        }),
        1
    );

    assert_eq!(
        events_emitted_by(&f.env, || {
            f.client
                .review_portfolio(&f.reviewer, &f.artist, &quality(90), &note(&f.env));
        }),
        1
    );

    assert_eq!(
        events_emitted_by(&f.env, || {
            f.client.issue_badge(
                &f.reviewer,
                &f.artist,
                &BadgeType::IdVerified,
                &0,
                &note(&f.env),
            );
        }),
        1
    );

    assert_eq!(
        events_emitted_by(&f.env, || {
            f.client.revoke_badge(
                &f.reviewer,
                &f.artist,
                &BadgeType::IdVerified,
                &note(&f.env),
            );
        }),
        1
    );
}
