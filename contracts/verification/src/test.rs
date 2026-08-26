extern crate std;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

use crate::errors::VerificationError;
use crate::types::{PortfolioStatus, QualityScore, ReviewOutcome};
use crate::{Verification, VerificationClient};

const MIN_SCORE: u32 = 70;
const MIN_WORK_COUNT: u32 = 3;
const UPDATE_INTERVAL: u32 = 1000;
const HISTORY_LIMIT: u32 = 3;

struct Fixture<'a> {
    env: Env,
    client: VerificationClient<'a>,
    admin: Address,
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
        admin,
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

#[test]
fn initialize_twice_fails() {
    let f = setup();
    let err = f
        .client
        .try_initialize(
            &f.admin,
            &MIN_SCORE,
            &MIN_WORK_COUNT,
            &UPDATE_INTERVAL,
            &HISTORY_LIMIT,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VerificationError::AlreadyInitialized);
}

#[test]
fn submit_review_and_approve() {
    let f = setup();
    f.client.submit_portfolio(&f.artist, &uri(&f.env), &5);

    let portfolio = f.client.get_portfolio(&f.artist);
    assert_eq!(portfolio.status, PortfolioStatus::Submitted);
    assert_eq!(portfolio.revision, 1);

    f.client.start_review(&f.reviewer, &f.artist);
    assert_eq!(
        f.client.get_portfolio(&f.artist).status,
        PortfolioStatus::UnderReview
    );

    let score = f
        .client
        .review_portfolio(&f.reviewer, &f.artist, &quality(80), &note(&f.env));
    assert_eq!(score, 80);

    let portfolio = f.client.get_portfolio(&f.artist);
    assert_eq!(portfolio.status, PortfolioStatus::Verified);
    assert_eq!(portfolio.score, 80);
    assert_eq!(portfolio.reviewer, Some(f.reviewer.clone()));
    assert!(f.client.is_verified(&f.artist));
    assert!(!f.client.requires_update(&f.artist));
}

#[test]
fn weighted_score_blends_criteria() {
    let f = setup();
    f.client.submit_portfolio(&f.artist, &uri(&f.env), &5);
    f.client.start_review(&f.reviewer, &f.artist);

    // 100*30 + 50*30 + 60*20 + 40*20 = 6500 -> 65
    let score = f.client.review_portfolio(
        &f.reviewer,
        &f.artist,
        &QualityScore {
            originality: 100,
            technique: 50,
            consistency: 60,
            presentation: 40,
        },
        &note(&f.env),
    );
    assert_eq!(score, 65);
    assert_eq!(
        f.client.get_portfolio(&f.artist).status,
        PortfolioStatus::Rejected
    );
    assert!(!f.client.is_verified(&f.artist));
}

#[test]
fn score_above_scale_is_rejected() {
    let f = setup();
    f.client.submit_portfolio(&f.artist, &uri(&f.env), &5);
    f.client.start_review(&f.reviewer, &f.artist);
    let err = f
        .client
        .try_review_portfolio(&f.reviewer, &f.artist, &quality(101), &note(&f.env))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VerificationError::InvalidScore);
}

#[test]
fn work_count_below_minimum_is_rejected() {
    let f = setup();
    let err = f
        .client
        .try_submit_portfolio(&f.artist, &uri(&f.env), &2)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VerificationError::InvalidWorkCount);
}

#[test]
fn duplicate_submission_is_rejected() {
    let f = setup();
    f.client.submit_portfolio(&f.artist, &uri(&f.env), &5);
    let err = f
        .client
        .try_submit_portfolio(&f.artist, &uri(&f.env), &5)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VerificationError::PortfolioExists);
}

#[test]
fn review_requires_claimed_portfolio() {
    let f = setup();
    f.client.submit_portfolio(&f.artist, &uri(&f.env), &5);
    let err = f
        .client
        .try_review_portfolio(&f.reviewer, &f.artist, &quality(90), &note(&f.env))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VerificationError::InvalidStatus);
}

#[test]
fn non_reviewer_cannot_review() {
    let f = setup();
    let outsider = Address::generate(&f.env);
    f.client.submit_portfolio(&f.artist, &uri(&f.env), &5);
    let err = f
        .client
        .try_start_review(&outsider, &f.artist)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VerificationError::Unauthorized);

    f.client.remove_reviewer(&f.reviewer);
    let err = f
        .client
        .try_start_review(&f.reviewer, &f.artist)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VerificationError::Unauthorized);
}

#[test]
fn update_resets_verification_and_records_history() {
    let f = setup();
    f.client.submit_portfolio(&f.artist, &uri(&f.env), &5);
    f.client.start_review(&f.reviewer, &f.artist);
    f.client
        .review_portfolio(&f.reviewer, &f.artist, &quality(90), &note(&f.env));
    assert!(f.client.is_verified(&f.artist));

    f.client.update_portfolio(
        &f.artist,
        &String::from_str(&f.env, "ipfs://portfolio-v2"),
        &7,
    );
    let portfolio = f.client.get_portfolio(&f.artist);
    assert_eq!(portfolio.status, PortfolioStatus::Submitted);
    assert_eq!(portfolio.revision, 2);
    assert_eq!(portfolio.work_count, 7);
    assert_eq!(portfolio.next_update_ledger, 0);
    assert!(!f.client.is_verified(&f.artist));

    let history = f.client.get_history(&f.artist);
    assert_eq!(history.len(), 2);
    assert_eq!(history.get(0).unwrap().outcome, ReviewOutcome::Approved);
    assert_eq!(history.get(1).unwrap().outcome, ReviewOutcome::Resubmitted);
}

#[test]
fn history_is_capped_at_the_configured_limit() {
    let f = setup();
    f.client.submit_portfolio(&f.artist, &uri(&f.env), &5);
    for _ in 0..HISTORY_LIMIT + 2 {
        f.client.start_review(&f.reviewer, &f.artist);
        f.client
            .review_portfolio(&f.reviewer, &f.artist, &quality(40), &note(&f.env));
        f.client.update_portfolio(&f.artist, &uri(&f.env), &5);
    }
    assert_eq!(f.client.get_history(&f.artist).len(), HISTORY_LIMIT);
}

#[test]
fn verification_goes_stale_and_can_be_flagged() {
    let f = setup();
    f.client.submit_portfolio(&f.artist, &uri(&f.env), &5);
    f.client.start_review(&f.reviewer, &f.artist);
    f.client
        .review_portfolio(&f.reviewer, &f.artist, &quality(90), &note(&f.env));

    let err = f
        .client
        .try_flag_update_required(&f.artist)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VerificationError::UpdateNotDue);

    f.env
        .ledger()
        .with_mut(|l| l.sequence_number += UPDATE_INTERVAL + 1);
    assert!(!f.client.is_verified(&f.artist));
    assert!(f.client.requires_update(&f.artist));

    f.client.flag_update_required(&f.artist);
    assert_eq!(
        f.client.get_portfolio(&f.artist).status,
        PortfolioStatus::UpdateRequired
    );
}

#[test]
fn unknown_artist_is_not_verified() {
    let f = setup();
    let stranger = Address::generate(&f.env);
    assert!(!f.client.is_verified(&stranger));
    assert!(!f.client.requires_update(&stranger));
    let err = f
        .client
        .try_get_portfolio(&stranger)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VerificationError::PortfolioNotFound);
}

#[test]
fn admin_can_tighten_the_minimum_score() {
    let f = setup();
    f.client.set_min_score(&95);
    f.client.submit_portfolio(&f.artist, &uri(&f.env), &5);
    f.client.start_review(&f.reviewer, &f.artist);
    f.client
        .review_portfolio(&f.reviewer, &f.artist, &quality(90), &note(&f.env));
    assert_eq!(
        f.client.get_portfolio(&f.artist).status,
        PortfolioStatus::Rejected
    );
}
