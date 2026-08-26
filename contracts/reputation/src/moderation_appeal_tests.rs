//! Review moderation & appeal system tests — closes #604.

#![cfg(test)]

extern crate std;

use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, String};

use crate::{
    errors::ReputationError,
    types::{AppealStatus, ReportReason, ReviewStatus},
    ReputationContract, ReputationContractClient,
};

fn make_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env
}

fn make_bytes(env: &Env, s: &str) -> Bytes {
    Bytes::from_slice(env, s.as_bytes())
}

fn make_str(env: &Env, s: &str) -> String {
    String::from_str(env, s)
}

struct Setup {
    client: ReputationContractClient<'static>,
    admin: Address,
    artist: Address,
    reviewer: Address,
    review_id: Bytes,
}

fn setup(env: &Env) -> Setup {
    let contract_id = env.register_contract(None, ReputationContract);
    let client = ReputationContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let artist = Address::generate(env);
    let reviewer = Address::generate(env);

    client.initialize(&admin).unwrap();
    let review_id = make_bytes(env, "rev001");
    client
        .submit_review(
            &review_id,
            &artist,
            &reviewer,
            &75u32,
            &make_str(env, "Good work"),
        )
        .unwrap();

    Setup { client, admin, artist, reviewer, review_id }
}

// ---------------------------------------------------------------------------
// report_review
// ---------------------------------------------------------------------------

#[test]
fn report_review_happy_path() {
    let env = make_env();
    let s = setup(&env);

    let report_id = make_bytes(&env, "rep001");
    let reporter = Address::generate(&env);
    s.client
        .report_review(
            &report_id,
            &s.review_id,
            &reporter,
            &ReportReason::Spam,
            &make_str(&env, "Looks like spam"),
        )
        .unwrap();

    let reports = s.client.get_reports(&s.review_id);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports.get(0).unwrap().reason, ReportReason::Spam);
}

#[test]
fn report_review_adds_to_moderation_queue() {
    let env = make_env();
    let s = setup(&env);

    s.client
        .report_review(
            &make_bytes(&env, "rep001"),
            &s.review_id,
            &Address::generate(&env),
            &ReportReason::Abuse,
            &make_str(&env, "abusive content"),
        )
        .unwrap();

    let queue = s.client.get_moderation_queue().unwrap();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.get(0).unwrap(), s.review_id);
}

#[test]
fn report_review_duplicate_report_id_rejected() {
    let env = make_env();
    let s = setup(&env);

    let report_id = make_bytes(&env, "rep001");
    s.client
        .report_review(
            &report_id,
            &s.review_id,
            &Address::generate(&env),
            &ReportReason::Spam,
            &make_str(&env, "spam"),
        )
        .unwrap();

    let err = s.client
        .report_review(
            &report_id, // same report_id
            &s.review_id,
            &Address::generate(&env),
            &ReportReason::Other,
            &make_str(&env, "other"),
        )
        .unwrap_err();
    assert_eq!(err, ReputationError::DuplicateReport);
}

#[test]
fn multiple_reporters_same_review_allowed() {
    let env = make_env();
    let s = setup(&env);

    s.client
        .report_review(
            &make_bytes(&env, "rep001"),
            &s.review_id,
            &Address::generate(&env),
            &ReportReason::Spam,
            &make_str(&env, "spam"),
        )
        .unwrap();

    s.client
        .report_review(
            &make_bytes(&env, "rep002"),
            &s.review_id,
            &Address::generate(&env),
            &ReportReason::Abuse,
            &make_str(&env, "abuse"),
        )
        .unwrap();

    let reports = s.client.get_reports(&s.review_id);
    assert_eq!(reports.len(), 2);
}

// ---------------------------------------------------------------------------
// Moderation history
// ---------------------------------------------------------------------------

#[test]
fn moderate_review_records_history() {
    let env = make_env();
    let s = setup(&env);

    s.client.moderate_review(&s.review_id).unwrap();

    let history = s.client.get_moderation_history(&s.review_id);
    assert_eq!(history.len(), 1);
}

#[test]
fn resolve_dispute_records_history() {
    let env = make_env();
    let s = setup(&env);

    s.client.open_dispute(&s.review_id, &s.artist).unwrap();
    s.client.resolve_dispute(&s.review_id, &true).unwrap();

    let history = s.client.get_moderation_history(&s.review_id);
    assert_eq!(history.len(), 1);
}

// ---------------------------------------------------------------------------
// submit_appeal
// ---------------------------------------------------------------------------

#[test]
fn appeal_after_moderation_happy_path() {
    let env = make_env();
    let s = setup(&env);

    s.client.moderate_review(&s.review_id).unwrap();

    let appeal_id = make_bytes(&env, "apl001");
    s.client
        .submit_appeal(
            &appeal_id,
            &s.review_id,
            &s.artist,
            &make_str(&env, "The review was legitimate"),
        )
        .unwrap();

    let appeal = s.client.get_appeal(&appeal_id).unwrap();
    assert_eq!(appeal.status, AppealStatus::Pending);
    assert_eq!(appeal.review_id, s.review_id);
}

#[test]
fn appeal_on_active_review_rejected() {
    let env = make_env();
    let s = setup(&env);

    // Review is still Active — appeal not allowed.
    let err = s.client
        .submit_appeal(
            &make_bytes(&env, "apl001"),
            &s.review_id,
            &s.artist,
            &make_str(&env, "test"),
        )
        .unwrap_err();
    assert_eq!(err, ReputationError::InvalidReviewState);
}

#[test]
fn appeal_by_unauthorized_address_rejected() {
    let env = make_env();
    let s = setup(&env);

    s.client.moderate_review(&s.review_id).unwrap();

    let stranger = Address::generate(&env);
    let err = s.client
        .submit_appeal(
            &make_bytes(&env, "apl001"),
            &s.review_id,
            &stranger,
            &make_str(&env, "unfair"),
        )
        .unwrap_err();
    assert_eq!(err, ReputationError::Unauthorized);
}

// ---------------------------------------------------------------------------
// resolve_appeal
// ---------------------------------------------------------------------------

#[test]
fn resolve_appeal_accept_reinstates_review() {
    let env = make_env();
    let s = setup(&env);

    s.client.moderate_review(&s.review_id).unwrap();
    let appeal_id = make_bytes(&env, "apl001");
    s.client
        .submit_appeal(
            &appeal_id,
            &s.review_id,
            &s.artist,
            &make_str(&env, "Unjust moderation"),
        )
        .unwrap();

    s.client.resolve_appeal(&appeal_id, &true).unwrap();

    let review = s.client.get_review(&s.review_id).unwrap();
    assert_eq!(review.status, ReviewStatus::Active);

    let appeal = s.client.get_appeal(&appeal_id).unwrap();
    assert_eq!(appeal.status, AppealStatus::Accepted);

    // History should have Hidden + AppealAccepted
    let history = s.client.get_moderation_history(&s.review_id);
    assert_eq!(history.len(), 2);
}

#[test]
fn resolve_appeal_reject_keeps_moderated() {
    let env = make_env();
    let s = setup(&env);

    s.client.moderate_review(&s.review_id).unwrap();
    let appeal_id = make_bytes(&env, "apl001");
    s.client
        .submit_appeal(
            &appeal_id,
            &s.review_id,
            &s.artist,
            &make_str(&env, "appeal reason"),
        )
        .unwrap();

    s.client.resolve_appeal(&appeal_id, &false).unwrap();

    let review = s.client.get_review(&s.review_id).unwrap();
    assert_eq!(review.status, ReviewStatus::Moderated);

    let appeal = s.client.get_appeal(&appeal_id).unwrap();
    assert_eq!(appeal.status, AppealStatus::Rejected);
}

#[test]
fn resolve_appeal_twice_rejected() {
    let env = make_env();
    let s = setup(&env);

    s.client.moderate_review(&s.review_id).unwrap();
    let appeal_id = make_bytes(&env, "apl001");
    s.client
        .submit_appeal(
            &appeal_id,
            &s.review_id,
            &s.artist,
            &make_str(&env, "appeal reason"),
        )
        .unwrap();

    s.client.resolve_appeal(&appeal_id, &true).unwrap();

    let err = s.client.resolve_appeal(&appeal_id, &false).unwrap_err();
    assert_eq!(err, ReputationError::AppealNotPending);
}

// ---------------------------------------------------------------------------
// escalate_appeal
// ---------------------------------------------------------------------------

#[test]
fn escalate_appeal_happy_path() {
    let env = make_env();
    let s = setup(&env);

    s.client.moderate_review(&s.review_id).unwrap();
    let appeal_id = make_bytes(&env, "apl001");
    s.client
        .submit_appeal(
            &appeal_id,
            &s.review_id,
            &s.artist,
            &make_str(&env, "escalate please"),
        )
        .unwrap();

    s.client.escalate_appeal(&appeal_id).unwrap();

    let appeal = s.client.get_appeal(&appeal_id).unwrap();
    assert_eq!(appeal.status, AppealStatus::Escalated);

    let history = s.client.get_moderation_history(&s.review_id);
    // Hidden + Escalated
    assert_eq!(history.len(), 2);
}

#[test]
fn escalate_already_resolved_appeal_rejected() {
    let env = make_env();
    let s = setup(&env);

    s.client.moderate_review(&s.review_id).unwrap();
    let appeal_id = make_bytes(&env, "apl001");
    s.client
        .submit_appeal(
            &appeal_id,
            &s.review_id,
            &s.artist,
            &make_str(&env, "reason"),
        )
        .unwrap();

    s.client.resolve_appeal(&appeal_id, &true).unwrap(); // already resolved

    let err = s.client.escalate_appeal(&appeal_id).unwrap_err();
    assert_eq!(err, ReputationError::AppealNotPending);
}

// ---------------------------------------------------------------------------
// Error code stability
// ---------------------------------------------------------------------------

#[test]
fn appeal_error_codes_stable() {
    assert_eq!(ReputationError::DuplicateReport as u32, 11);
    assert_eq!(ReputationError::AppealNotFound as u32, 12);
    assert_eq!(ReputationError::AppealAlreadyOpen as u32, 13);
    assert_eq!(ReputationError::AppealNotPending as u32, 14);
    assert_eq!(ReputationError::InvalidReviewState as u32, 15);
}
