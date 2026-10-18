//! Event-count regression guard for the reputation contract (closes #994).
//!
//! Mirrors the pattern from `contracts/verification/tests/event_budgets.rs`.
//! Each operation must emit exactly the documented number of events; an extra
//! `publish` call added anywhere will make this test fail under `cargo test`
//! before it becomes a fee regression on-chain.
//!
//! Run with:
//!   cargo test -p reputation -- event_budgets

extern crate std;

use soroban_sdk::{
    testutils::{Address as _, Events as _},
    Address, Bytes, Env, String,
};

use reputation::types::{AppealStatus, ReportReason, ReviewStatus};
use reputation::{ReputationContract, ReputationContractClient};

/// How many contract events one call published.
///
/// Only the *difference* around the call is a meaningful event count — the
/// raw log length grows across the whole test.
fn events_emitted_by<F: FnOnce()>(env: &Env, call: F) -> u32 {
    let before = env.events().all().len();
    call();
    (env.events().all().len() - before) as u32
}

fn review_id(env: &Env, tag: &str) -> Bytes {
    Bytes::from_slice(env, tag.as_bytes())
}

fn note(env: &Env) -> String {
    String::from_str(env, "reviewed manually")
}

struct Fixture<'a> {
    env: Env,
    client: ReputationContractClient<'a>,
    admin: Address,
    artist: Address,
    reviewer: Address,
    reporter: Address,
}

fn setup<'a>() -> Fixture<'a> {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let admin = Address::generate(&env);
    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);
    let reporter = Address::generate(&env);

    let contract_id = env.register_contract(None, ReputationContract);
    let client = ReputationContractClient::new(&env, &contract_id);
    client.initialize(&admin).unwrap();

    Fixture {
        env,
        client,
        admin,
        artist,
        reviewer,
        reporter,
    }
}

/// `submit_review` publishes exactly 1 event (`review`).
#[test]
fn submit_review_emits_one_event() {
    let f = setup();
    let id = review_id(&f.env, "budget-submit");
    assert_eq!(
        events_emitted_by(&f.env, || {
            f.client
                .submit_review(&id, &f.artist, &f.reviewer, &85, &note(&f.env))
                .unwrap();
        }),
        1
    );
}

/// `report_review` publishes exactly 1 event (`reported`).
#[test]
fn report_review_emits_one_event() {
    let f = setup();
    let id = review_id(&f.env, "budget-report");
    f.client
        .submit_review(&id, &f.artist, &f.reviewer, &85, &note(&f.env))
        .unwrap();
    assert_eq!(
        events_emitted_by(&f.env, || {
            f.client
                .report_review(
                    &id,
                    &f.reporter,
                    &ReportReason::Spam,
                    &note(&f.env),
                )
                .unwrap();
        }),
        1
    );
}

/// `moderate_review` publishes exactly 1 event (`moderated`).
#[test]
fn moderate_review_emits_one_event() {
    let f = setup();
    let id = review_id(&f.env, "budget-moderate");
    f.client
        .submit_review(&id, &f.artist, &f.reviewer, &85, &note(&f.env))
        .unwrap();
    f.client
        .report_review(&id, &f.reporter, &ReportReason::Spam, &note(&f.env))
        .unwrap();
    assert_eq!(
        events_emitted_by(&f.env, || {
            f.client
                .moderate_review(&id, &ReviewStatus::Cleared, &note(&f.env))
                .unwrap();
        }),
        1
    );
}

/// `file_appeal` publishes exactly 1 event (`appeal`).
#[test]
fn file_appeal_emits_one_event() {
    let f = setup();
    let id = review_id(&f.env, "budget-appeal");
    f.client
        .submit_review(&id, &f.artist, &f.reviewer, &85, &note(&f.env))
        .unwrap();
    f.client
        .report_review(&id, &f.reporter, &ReportReason::Spam, &note(&f.env))
        .unwrap();
    f.client
        .moderate_review(&id, &ReviewStatus::Removed, &note(&f.env))
        .unwrap();
    assert_eq!(
        events_emitted_by(&f.env, || {
            f.client
                .file_appeal(&id, &f.artist, &note(&f.env))
                .unwrap();
        }),
        1
    );
}

/// `resolve_appeal` publishes exactly 1 event (`appeal_rs`).
#[test]
fn resolve_appeal_emits_one_event() {
    let f = setup();
    let id = review_id(&f.env, "budget-resolve");
    f.client
        .submit_review(&id, &f.artist, &f.reviewer, &85, &note(&f.env))
        .unwrap();
    f.client
        .report_review(&id, &f.reporter, &ReportReason::Spam, &note(&f.env))
        .unwrap();
    f.client
        .moderate_review(&id, &ReviewStatus::Removed, &note(&f.env))
        .unwrap();
    f.client
        .file_appeal(&id, &f.artist, &note(&f.env))
        .unwrap();
    assert_eq!(
        events_emitted_by(&f.env, || {
            f.client
                .resolve_appeal(&id, &AppealStatus::Upheld)
                .unwrap();
        }),
        1
    );
}
