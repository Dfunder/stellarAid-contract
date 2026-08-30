extern crate std;

use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, String};

use crate::{
    ReputationContract, ReputationContractClient,
    errors::ReputationError,
    types::{AppealStatus, ReportReason, ReviewStatus},
};

fn setup(env: &Env) -> (ReputationContractClient, Address) {
    let contract_id = env.register_contract(None, ReputationContract);
    let client = ReputationContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

fn make_bytes(env: &Env, s: &str) -> Bytes {
    Bytes::from_slice(env, s.as_bytes())
}

fn make_string(env: &Env, s: &str) -> String {
    String::from_str(env, s)
}

// ── Initialization ─────────────────────────────────────────────────────────

#[test]
fn test_initialize_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    setup(&env);
}

#[test]
fn test_double_initialize_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let result = client.try_initialize(&admin);
    assert_eq!(result, Err(Ok(ReputationError::AlreadyInitialized)));
}

// ── Review submission ──────────────────────────────────────────────────────

#[test]
fn test_submit_review_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);
    let review_id = make_bytes(&env, "r1");

    client.submit_review(
        &review_id,
        &artist,
        &reviewer,
        &45u32,
        &make_string(&env, "Great work!"),
    );

    let r = client.get_review(&review_id);
    assert_eq!(r.status, ReviewStatus::Active);
    assert_eq!(r.rating_x10, 45);
    assert_eq!(r.artist, artist);
}

#[test]
fn test_submit_review_invalid_rating_too_low_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);
    let result = client.try_submit_review(
        &make_bytes(&env, "r2"),
        &artist,
        &reviewer,
        &5u32,
        &make_string(&env, "bad"),
    );
    assert_eq!(result, Err(Ok(ReputationError::InvalidRating)));
}

#[test]
fn test_submit_review_invalid_rating_too_high_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);
    let result = client.try_submit_review(
        &make_bytes(&env, "r3"),
        &artist,
        &reviewer,
        &55u32,
        &make_string(&env, "bad"),
    );
    assert_eq!(result, Err(Ok(ReputationError::InvalidRating)));
}

#[test]
fn test_submit_review_boundary_ratings_ok() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);
    // Min
    client.submit_review(
        &make_bytes(&env, "r4"),
        &artist,
        &reviewer,
        &10u32,
        &make_string(&env, "min"),
    );
    // Max
    client.submit_review(
        &make_bytes(&env, "r5"),
        &artist,
        &reviewer,
        &50u32,
        &make_string(&env, "max"),
    );
}

// ── Review reporting ───────────────────────────────────────────────────────

#[test]
fn test_report_review_transitions_to_under_review() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);
    let reporter = Address::generate(&env);
    let review_id = make_bytes(&env, "rep1");

    client.submit_review(
        &review_id,
        &artist,
        &reviewer,
        &30u32,
        &make_string(&env, "ok"),
    );

    client.report_review(
        &review_id,
        &reporter,
        &ReportReason::Spam,
        &make_string(&env, "this is spam"),
    );

    let r = client.get_review(&review_id);
    assert_eq!(r.status, ReviewStatus::UnderReview);
}

#[test]
fn test_report_review_increments_count() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);
    let reporter1 = Address::generate(&env);
    let reporter2 = Address::generate(&env);
    let review_id = make_bytes(&env, "rep2");

    client.submit_review(&review_id, &artist, &reviewer, &40u32, &make_string(&env, "ok"));
    client.report_review(&review_id, &reporter1, &ReportReason::Abuse, &make_string(&env, "a"));
    // After first report review is UnderReview; second reporter can still add
    // a report from a different address but status stays UnderReview
    // (note: re-report by same reporter should fail)
    let result = client.try_report_review(
        &review_id, &reporter1, &ReportReason::Abuse, &make_string(&env, "dup"),
    );
    assert_eq!(result, Err(Ok(ReputationError::AlreadyReported)));

    // Different reporter also cannot report a UnderReview - only Active/Cleared
    let result2 = client.try_report_review(
        &review_id, &reporter2, &ReportReason::Misleading, &make_string(&env, "b"),
    );
    assert_eq!(result2, Err(Ok(ReputationError::InvalidStatus)));
}

#[test]
fn test_report_removed_review_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);
    let reporter = Address::generate(&env);
    let review_id = make_bytes(&env, "rep3");

    client.submit_review(&review_id, &artist, &reviewer, &20u32, &make_string(&env, "meh"));
    client.report_review(&review_id, &reporter, &ReportReason::Spam, &make_string(&env, "s"));
    client.moderate_review(&review_id, &ReviewStatus::Removed, &make_string(&env, "spam"));

    let reporter2 = Address::generate(&env);
    let result = client.try_report_review(
        &review_id, &reporter2, &ReportReason::Other, &make_string(&env, "x"),
    );
    assert_eq!(result, Err(Ok(ReputationError::InvalidStatus)));
}

// ── Admin moderation ───────────────────────────────────────────────────────

#[test]
fn test_moderate_review_remove() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);
    let reporter = Address::generate(&env);
    let review_id = make_bytes(&env, "mod1");

    client.submit_review(&review_id, &artist, &reviewer, &30u32, &make_string(&env, "ok"));
    client.report_review(&review_id, &reporter, &ReportReason::Spam, &make_string(&env, "s"));
    client.moderate_review(&review_id, &ReviewStatus::Removed, &make_string(&env, "spam confirmed"));

    assert_eq!(client.get_review(&review_id).status, ReviewStatus::Removed);
    assert_eq!(client.get_moderation_count(&review_id), 1);
    let entry = client.get_moderation_entry(&review_id, &0);
    assert_eq!(entry.new_status, ReviewStatus::Removed);
}

#[test]
fn test_moderate_review_clear() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);
    let reporter = Address::generate(&env);
    let review_id = make_bytes(&env, "mod2");

    client.submit_review(&review_id, &artist, &reviewer, &45u32, &make_string(&env, "fine"));
    client.report_review(&review_id, &reporter, &ReportReason::Other, &make_string(&env, "o"));
    client.moderate_review(&review_id, &ReviewStatus::Cleared, &make_string(&env, "no violation"));

    assert_eq!(client.get_review(&review_id).status, ReviewStatus::Cleared);
}

#[test]
fn test_moderate_non_under_review_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);
    let review_id = make_bytes(&env, "mod3");

    client.submit_review(&review_id, &artist, &reviewer, &40u32, &make_string(&env, "nice"));

    // Status is Active, not UnderReview
    let result = client.try_moderate_review(
        &review_id, &ReviewStatus::Removed, &make_string(&env, "no"),
    );
    assert_eq!(result, Err(Ok(ReputationError::InvalidStatus)));
}

#[test]
fn test_moderation_queue_size_increments() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);
    let reporter = Address::generate(&env);

    let r1 = make_bytes(&env, "q1");
    let r2 = make_bytes(&env, "q2");

    client.submit_review(&r1, &artist, &reviewer, &30u32, &make_string(&env, "a"));
    client.submit_review(&r2, &artist, &reviewer, &40u32, &make_string(&env, "b"));

    client.report_review(&r1, &reporter, &ReportReason::Spam, &make_string(&env, "x"));
    let reporter2 = Address::generate(&env);
    client.report_review(&r2, &reporter2, &ReportReason::Abuse, &make_string(&env, "y"));

    assert_eq!(client.get_moderation_queue_size(), 2);
}

// ── Appeal mechanism ───────────────────────────────────────────────────────

#[test]
fn test_file_and_resolve_appeal_upheld() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);
    let reporter = Address::generate(&env);
    let review_id = make_bytes(&env, "ap1");

    client.submit_review(&review_id, &artist, &reviewer, &35u32, &make_string(&env, "good"));
    client.report_review(&review_id, &reporter, &ReportReason::Spam, &make_string(&env, "s"));
    client.moderate_review(&review_id, &ReviewStatus::Removed, &make_string(&env, "removed"));

    client.file_appeal(&review_id, &artist, &make_string(&env, "I disagree"));

    let appeal = client.get_appeal(&review_id);
    assert_eq!(appeal.status, AppealStatus::Pending);

    client.resolve_appeal(&review_id, &AppealStatus::Upheld);

    let appeal = client.get_appeal(&review_id);
    assert_eq!(appeal.status, AppealStatus::Upheld);

    // Review should be reinstated
    assert_eq!(client.get_review(&review_id).status, ReviewStatus::Active);
}

#[test]
fn test_file_and_resolve_appeal_denied() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);
    let reporter = Address::generate(&env);
    let review_id = make_bytes(&env, "ap2");

    client.submit_review(&review_id, &artist, &reviewer, &20u32, &make_string(&env, "meh"));
    client.report_review(&review_id, &reporter, &ReportReason::Abuse, &make_string(&env, "a"));
    client.moderate_review(&review_id, &ReviewStatus::Removed, &make_string(&env, "removed"));

    client.file_appeal(&review_id, &reviewer, &make_string(&env, "Not abuse"));

    client.resolve_appeal(&review_id, &AppealStatus::Denied);

    let appeal = client.get_appeal(&review_id);
    assert_eq!(appeal.status, AppealStatus::Denied);
    // Review stays Removed
    assert_eq!(client.get_review(&review_id).status, ReviewStatus::Removed);
}

#[test]
fn test_file_appeal_on_active_review_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);
    let review_id = make_bytes(&env, "ap3");

    client.submit_review(&review_id, &artist, &reviewer, &45u32, &make_string(&env, "ok"));

    let result = client.try_file_appeal(&review_id, &artist, &make_string(&env, "why?"));
    assert_eq!(result, Err(Ok(ReputationError::InvalidStatus)));
}

#[test]
fn test_duplicate_appeal_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);
    let reporter = Address::generate(&env);
    let review_id = make_bytes(&env, "ap4");

    client.submit_review(&review_id, &artist, &reviewer, &30u32, &make_string(&env, "ok"));
    client.report_review(&review_id, &reporter, &ReportReason::Spam, &make_string(&env, "s"));
    client.moderate_review(&review_id, &ReviewStatus::Removed, &make_string(&env, "removed"));

    client.file_appeal(&review_id, &artist, &make_string(&env, "reason 1"));
    let result = client.try_file_appeal(&review_id, &artist, &make_string(&env, "reason 2"));
    assert_eq!(result, Err(Ok(ReputationError::AppealAlreadyExists)));
}

#[test]
fn test_escalate_appeal() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);
    let reporter = Address::generate(&env);
    let review_id = make_bytes(&env, "ap5");

    client.submit_review(&review_id, &artist, &reviewer, &20u32, &make_string(&env, "meh"));
    client.report_review(&review_id, &reporter, &ReportReason::Abuse, &make_string(&env, "a"));
    client.moderate_review(&review_id, &ReviewStatus::Removed, &make_string(&env, "removed"));

    client.file_appeal(&review_id, &artist, &make_string(&env, "escalate please"));
    client.escalate_appeal(&review_id, &artist);

    assert_eq!(client.get_appeal(&review_id).status, AppealStatus::Escalated);
}

#[test]
fn test_third_party_cannot_file_appeal() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);
    let third_party = Address::generate(&env);
    let reporter = Address::generate(&env);
    let review_id = make_bytes(&env, "ap6");

    client.submit_review(&review_id, &artist, &reviewer, &30u32, &make_string(&env, "ok"));
    client.report_review(&review_id, &reporter, &ReportReason::Spam, &make_string(&env, "s"));
    client.moderate_review(&review_id, &ReviewStatus::Removed, &make_string(&env, "removed"));

    let result = client.try_file_appeal(&review_id, &third_party, &make_string(&env, "i want to appeal"));
    assert_eq!(result, Err(Ok(ReputationError::Unauthorized)));
}
