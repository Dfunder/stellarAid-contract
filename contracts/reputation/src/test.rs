#![cfg(test)]

extern crate std;

use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, String};

use crate::{errors::ReputationError, types::ReviewStatus, ReputationContract, ReputationContractClient};

fn make_env() -> Env {
    Env::default()
}

fn register(env: &Env) -> (Address, ReputationContractClient) {
    let contract_id = env.register_contract(None, ReputationContract);
    let client = ReputationContractClient::new(env, &contract_id);
    (contract_id, client)
}

fn make_bytes(env: &Env, s: &str) -> Bytes {
    Bytes::from_slice(env, s.as_bytes())
}

fn make_str(env: &Env, s: &str) -> String {
    String::from_str(env, s)
}

// ---------------------------------------------------------------------------
// Init tests
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_once() {
    let env = make_env();
    let (_, client) = register(&env);
    let admin = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin).unwrap();

    // Second init must fail.
    let err = client.initialize(&admin).unwrap_err();
    assert_eq!(err, ReputationError::AlreadyInitialized);
}

// ---------------------------------------------------------------------------
// Submit review tests
// ---------------------------------------------------------------------------

#[test]
fn test_submit_review_happy_path() {
    let env = make_env();
    let (_, client) = register(&env);
    let admin = Address::generate(&env);
    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin).unwrap();

    client
        .submit_review(
            &make_bytes(&env, "rev001"),
            &artist,
            &reviewer,
            &80u32,
            &make_str(&env, "Great work!"),
        )
        .unwrap();

    let stats = client.get_artist_stats(&artist);
    assert_eq!(stats.review_count, 1);
    assert_eq!(stats.total_score, 80);
    assert!(stats.reputation_score > 0);
}

#[test]
fn test_submit_review_invalid_rating() {
    let env = make_env();
    let (_, client) = register(&env);
    let admin = Address::generate(&env);
    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin).unwrap();

    // rating = 0 → invalid
    let err = client
        .submit_review(
            &make_bytes(&env, "rev_bad"),
            &artist,
            &reviewer,
            &0u32,
            &make_str(&env, "zero"),
        )
        .unwrap_err();
    assert_eq!(err, ReputationError::InvalidRating);

    // rating = 101 → invalid
    let err2 = client
        .submit_review(
            &make_bytes(&env, "rev_bad2"),
            &artist,
            &reviewer,
            &101u32,
            &make_str(&env, "over"),
        )
        .unwrap_err();
    assert_eq!(err2, ReputationError::InvalidRating);
}

#[test]
fn test_duplicate_review_prevented() {
    let env = make_env();
    let (_, client) = register(&env);
    let admin = Address::generate(&env);
    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin).unwrap();

    client
        .submit_review(
            &make_bytes(&env, "rev001"),
            &artist,
            &reviewer,
            &70u32,
            &make_str(&env, "Good"),
        )
        .unwrap();

    let err = client
        .submit_review(
            &make_bytes(&env, "rev002"),
            &artist,
            &reviewer,
            &90u32,
            &make_str(&env, "Great"),
        )
        .unwrap_err();

    assert_eq!(err, ReputationError::DuplicateReview);
}

// ---------------------------------------------------------------------------
// Moderation tests
// ---------------------------------------------------------------------------

#[test]
fn test_moderate_review() {
    let env = make_env();
    let (_, client) = register(&env);
    let admin = Address::generate(&env);
    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin).unwrap();
    client
        .submit_review(
            &make_bytes(&env, "rev001"),
            &artist,
            &reviewer,
            &60u32,
            &make_str(&env, "ok"),
        )
        .unwrap();

    client.moderate_review(&make_bytes(&env, "rev001")).unwrap();

    let review = client.get_review(&make_bytes(&env, "rev001")).unwrap();
    assert_eq!(review.status, ReviewStatus::Moderated);

    // Stats should be back to zero after moderation.
    let stats = client.get_artist_stats(&artist);
    assert_eq!(stats.review_count, 0);
}

// ---------------------------------------------------------------------------
// Dispute tests
// ---------------------------------------------------------------------------

#[test]
fn test_dispute_and_reinstate() {
    let env = make_env();
    let (_, client) = register(&env);
    let admin = Address::generate(&env);
    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin).unwrap();
    client
        .submit_review(
            &make_bytes(&env, "rev001"),
            &artist,
            &reviewer,
            &50u32,
            &make_str(&env, "so-so"),
        )
        .unwrap();

    // Artist disputes
    client.open_dispute(&make_bytes(&env, "rev001"), &artist).unwrap();
    let stats = client.get_artist_stats(&artist);
    assert_eq!(stats.review_count, 0); // removed during dispute

    // Admin reinstates
    client.resolve_dispute(&make_bytes(&env, "rev001"), &true).unwrap();
    let stats2 = client.get_artist_stats(&artist);
    assert_eq!(stats2.review_count, 1);
}

#[test]
fn test_dispute_and_reject() {
    let env = make_env();
    let (_, client) = register(&env);
    let admin = Address::generate(&env);
    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin).unwrap();
    client
        .submit_review(
            &make_bytes(&env, "rev001"),
            &artist,
            &reviewer,
            &50u32,
            &make_str(&env, "so-so"),
        )
        .unwrap();

    client.open_dispute(&make_bytes(&env, "rev001"), &artist).unwrap();
    // Admin rejects reinstatement → stays Moderated
    client.resolve_dispute(&make_bytes(&env, "rev001"), &false).unwrap();

    let review = client.get_review(&make_bytes(&env, "rev001")).unwrap();
    assert_eq!(review.status, ReviewStatus::Moderated);
    let stats = client.get_artist_stats(&artist);
    assert_eq!(stats.review_count, 0);
}

// ---------------------------------------------------------------------------
// Unauthorised dispute test
// ---------------------------------------------------------------------------

#[test]
fn test_dispute_unauthorized() {
    let env = make_env();
    let (_, client) = register(&env);
    let admin = Address::generate(&env);
    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);
    let stranger = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin).unwrap();
    client
        .submit_review(
            &make_bytes(&env, "rev001"),
            &artist,
            &reviewer,
            &70u32,
            &make_str(&env, "fine"),
        )
        .unwrap();

    let err = client
        .open_dispute(&make_bytes(&env, "rev001"), &stranger)
        .unwrap_err();
    assert_eq!(err, ReputationError::Unauthorized);
}
