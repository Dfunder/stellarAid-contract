extern crate std;

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Bytes, Env, String,
};

use crate::{
    ReputationContract, ReputationContractClient,
    errors::ReputationError,
    types::{
        AppealStatus, ReportReason, ReviewStatus, DEFAULT_APPEAL_WINDOW_LEDGERS,
        MAX_APPEAL_WINDOW_LEDGERS, MAX_QUEUE_SIZE,
    },
};

fn setup(env: &Env) -> (ReputationContractClient<'_>, Address) {
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

// ── Reputation aggregate (#974) ─────────────────────────────────────────────

#[test]
fn test_score_reflects_submitted_reviews() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    client.submit_review(
        &make_bytes(&env, "s1"),
        &artist,
        &Address::generate(&env),
        &50u32,
        &make_string(&env, "great"),
    );
    client.submit_review(
        &make_bytes(&env, "s2"),
        &artist,
        &Address::generate(&env),
        &30u32,
        &make_string(&env, "ok"),
    );

    let score = client.get_score(&artist);
    assert_eq!(score.total_reviews, 2);
    assert_eq!(score.active_reviews, 2);
    assert_eq!(score.rating_sum, 80);
    assert_eq!(score.average_rating_x10, 40);
    assert_eq!(score.removed_reviews, 0);
    assert_eq!(score.appeal_count, 0);
}

#[test]
fn test_score_of_unknown_subject_is_neutral() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let score = client.get_score(&Address::generate(&env));
    assert_eq!(score.total_reviews, 0);
    assert_eq!(score.average_rating_x10, 0);
    // The weighted score floors at the neutral prior (3.0), never below.
    assert_eq!(score.weighted_score_x10, 30);
}

#[test]
fn test_moderation_removes_a_review_from_the_score() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let review_id = make_bytes(&env, "m1");
    client.submit_review(&review_id, &artist, &Address::generate(&env), &50u32, &make_string(&env, "great"));
    client.report_review(&review_id, &Address::generate(&env), &ReportReason::Spam, &make_string(&env, "s"));
    client.moderate_review(&review_id, &ReviewStatus::Removed, &make_string(&env, "removed"));

    let score = client.get_score(&artist);
    assert_eq!(score.active_reviews, 0);
    assert_eq!(score.removed_reviews, 1);
    assert_eq!(score.average_rating_x10, 0);
    assert_eq!(score.moderation_count, 1);
}

#[test]
fn test_upheld_appeal_restores_the_score() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let review_id = make_bytes(&env, "m2");
    client.submit_review(&review_id, &artist, &Address::generate(&env), &40u32, &make_string(&env, "good"));
    client.report_review(&review_id, &Address::generate(&env), &ReportReason::Abuse, &make_string(&env, "a"));
    client.moderate_review(&review_id, &ReviewStatus::Removed, &make_string(&env, "removed"));
    assert_eq!(client.get_score(&artist).active_reviews, 0);

    client.file_appeal(&review_id, &artist, &make_string(&env, "unfair"));
    client.resolve_appeal(&review_id, &AppealStatus::Upheld);

    let score = client.get_score(&artist);
    assert_eq!(score.active_reviews, 1);
    assert_eq!(score.rating_sum, 40);
    assert_eq!(score.average_rating_x10, 40);
    assert_eq!(score.removed_reviews, 0);
    assert_eq!(score.appeal_count, 1);
}

// ── Appeal window (#976) ────────────────────────────────────────────────────

#[test]
fn test_appeal_window_default_is_reported() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    assert_eq!(client.get_appeal_window(), DEFAULT_APPEAL_WINDOW_LEDGERS);
}

#[test]
fn test_appeal_deadline_is_review_ledger_plus_window() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let review_id = make_bytes(&env, "w1");
    client.submit_review(&review_id, &artist, &Address::generate(&env), &30u32, &make_string(&env, "ok"));

    assert_eq!(client.get_appeal_deadline(&review_id), DEFAULT_APPEAL_WINDOW_LEDGERS);
    assert_eq!(client.ledgers_to_appeal(&review_id), DEFAULT_APPEAL_WINDOW_LEDGERS);
}

#[test]
fn test_file_appeal_at_the_boundary_is_allowed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let review_id = make_bytes(&env, "w2");
    client.submit_review(&review_id, &artist, &Address::generate(&env), &30u32, &make_string(&env, "ok"));
    client.report_review(&review_id, &Address::generate(&env), &ReportReason::Spam, &make_string(&env, "s"));
    client.moderate_review(&review_id, &ReviewStatus::Removed, &make_string(&env, "removed"));

    env.ledger().with_mut(|l| l.sequence_number = DEFAULT_APPEAL_WINDOW_LEDGERS);
    client.file_appeal(&review_id, &artist, &make_string(&env, "just in time"));
    assert_eq!(client.get_appeal(&review_id).status, AppealStatus::Pending);
}

#[test]
fn test_file_appeal_past_the_window_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let review_id = make_bytes(&env, "w3");
    client.submit_review(&review_id, &artist, &Address::generate(&env), &30u32, &make_string(&env, "ok"));
    client.report_review(&review_id, &Address::generate(&env), &ReportReason::Spam, &make_string(&env, "s"));
    client.moderate_review(&review_id, &ReviewStatus::Removed, &make_string(&env, "removed"));

    env.ledger().with_mut(|l| l.sequence_number = DEFAULT_APPEAL_WINDOW_LEDGERS + 1);
    let result = client.try_file_appeal(&review_id, &artist, &make_string(&env, "too late"));
    assert_eq!(result, Err(Ok(ReputationError::AppealWindowClosed)));
}

#[test]
fn test_set_appeal_window_rejects_out_of_range() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);

    for bad in [0u32, MAX_APPEAL_WINDOW_LEDGERS + 1] {
        let result = client.try_set_appeal_window(&admin, &bad);
        assert_eq!(result, Err(Ok(ReputationError::InvalidAppealWindow)));
    }
    client.set_appeal_window(&admin, &100u32);
    assert_eq!(client.get_appeal_window(), 100);
}

#[test]
fn test_a_custom_window_bounds_filing() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    client.set_appeal_window(&admin, &10u32);

    let artist = Address::generate(&env);
    let review_id = make_bytes(&env, "w4");
    client.submit_review(&review_id, &artist, &Address::generate(&env), &30u32, &make_string(&env, "ok"));
    client.report_review(&review_id, &Address::generate(&env), &ReportReason::Spam, &make_string(&env, "s"));
    client.moderate_review(&review_id, &ReviewStatus::Removed, &make_string(&env, "removed"));

    env.ledger().with_mut(|l| l.sequence_number = 11);
    let result = client.try_file_appeal(&review_id, &artist, &make_string(&env, "late"));
    assert_eq!(result, Err(Ok(ReputationError::AppealWindowClosed)));
}

// ── Moderation-queue bound (#976) ───────────────────────────────────────────

#[test]
fn test_per_review_report_cap_is_enforced() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let review_id = make_bytes(&env, "cap");
    client.submit_review(&review_id, &artist, &Address::generate(&env), &30u32, &make_string(&env, "ok"));

    // Each report moves the review to UnderReview and moderation back to
    // Cleared makes it reportable again, so the per-review cap is reachable.
    let mut reports = 0u32;
    while reports < crate::types::MAX_REPORTS_PER_REVIEW {
        client.report_review(&review_id, &Address::generate(&env), &ReportReason::Spam, &make_string(&env, "s"));
        client.moderate_review(&review_id, &ReviewStatus::Cleared, &make_string(&env, "cleared"));
        reports += 1;
    }

    let result = client.try_report_review(
        &review_id,
        &Address::generate(&env),
        &ReportReason::Spam,
        &make_string(&env, "s"),
    );
    assert_eq!(result, Err(Ok(ReputationError::ReportLimitReached)));
}

#[test]
fn test_queue_cap_rejects_new_reports() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    for i in 0..MAX_QUEUE_SIZE {
        let id = Bytes::from_slice(&env, format!("q{i}").as_bytes());
        client.submit_review(&id, &artist, &Address::generate(&env), &30u32, &make_string(&env, "ok"));
        client.report_review(&id, &Address::generate(&env), &ReportReason::Spam, &make_string(&env, "s"));
    }
    assert_eq!(client.get_moderation_queue_size(), MAX_QUEUE_SIZE);

    let extra = Bytes::from_slice(&env, b"q-extra");
    client.submit_review(&extra, &artist, &Address::generate(&env), &30u32, &make_string(&env, "ok"));
    let result = client.try_report_review(
        &extra,
        &Address::generate(&env),
        &ReportReason::Spam,
        &make_string(&env, "s"),
    );
    assert_eq!(result, Err(Ok(ReputationError::QueueFull)));
}

// ── Error-code stability ────────────────────────────────────────────────────

#[test]
fn test_error_codes_are_stable() {
    assert_eq!(ReputationError::AlreadyInitialized as u32, 1);
    assert_eq!(ReputationError::NotInitialized as u32, 2);
    assert_eq!(ReputationError::Unauthorized as u32, 3);
    assert_eq!(ReputationError::ReviewNotFound as u32, 4);
    assert_eq!(ReputationError::AlreadyReported as u32, 5);
    assert_eq!(ReputationError::InvalidRating as u32, 6);
    assert_eq!(ReputationError::InvalidStatus as u32, 7);
    assert_eq!(ReputationError::AppealAlreadyExists as u32, 8);
    assert_eq!(ReputationError::AppealNotFound as u32, 9);
    assert_eq!(ReputationError::ArithmeticOverflow as u32, 10);
    assert_eq!(ReputationError::DuplicateReview as u32, 11);
    assert_eq!(ReputationError::CommentTooLong as u32, 12);
    assert_eq!(ReputationError::AppealWindowClosed as u32, 13);
    assert_eq!(ReputationError::QueueFull as u32, 14);
    assert_eq!(ReputationError::ReportLimitReached as u32, 15);
    assert_eq!(ReputationError::InvalidAppealWindow as u32, 16);
}
