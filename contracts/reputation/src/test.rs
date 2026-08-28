use super::*;
use soroban_sdk::testutils::Address as _;

fn setup(env: &Env) -> (ReputationClient<'_>, Address) {
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Reputation);
    let client = ReputationClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

#[test]
fn submit_review_records_and_scores() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);

    let idx = client.submit_review(&reviewer, &artist, &5, &String::from_str(&env, "great"));
    assert_eq!(idx, 0);
    assert_eq!(client.get_review_count(&artist), 1);
    // One 5-star review, low confidence (1/5) -> 100 * 1/5 = 20.
    assert_eq!(client.get_reputation(&artist), 20);
}

#[test]
fn duplicate_review_from_same_client_is_rejected() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);

    client.submit_review(&reviewer, &artist, &4, &String::from_str(&env, "good"));
    let result = client.try_submit_review(&reviewer, &artist, &5, &String::from_str(&env, "again"));
    assert_eq!(result, Err(Ok(ReputationError::DuplicateReview)));
}

#[test]
fn out_of_range_rating_is_rejected() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);

    let result = client.try_submit_review(&reviewer, &artist, &0, &String::from_str(&env, "x"));
    assert_eq!(result, Err(Ok(ReputationError::InvalidRating)));

    let result = client.try_submit_review(&reviewer, &artist, &6, &String::from_str(&env, "x"));
    assert_eq!(result, Err(Ok(ReputationError::InvalidRating)));
}

#[test]
fn confidence_ramps_up_with_more_reviews() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let artist = Address::generate(&env);

    for _ in 0..5 {
        let reviewer = Address::generate(&env);
        client.submit_review(&reviewer, &artist, &5, &String::from_str(&env, "great"));
    }
    // 5 counted 5-star reviews reaches full confidence -> 100.
    assert_eq!(client.get_reputation(&artist), 100);
}

#[test]
fn recent_reviews_are_weighted_more_heavily() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let artist = Address::generate(&env);

    // Four low ratings followed by one high rating: the high, most-recent
    // rating should pull the weighted average up more than a plain mean would.
    for _ in 0..4 {
        let reviewer = Address::generate(&env);
        client.submit_review(&reviewer, &artist, &1, &String::from_str(&env, "meh"));
    }
    let last_reviewer = Address::generate(&env);
    client.submit_review(&last_reviewer, &artist, &5, &String::from_str(&env, "great"));

    // Plain mean would be (1+1+1+1+5)/5 = 1.8 -> 36 (at full confidence, 5 reviews).
    // Weighted mean (weights 1..5): (1*1+1*2+1*3+1*4+5*5)/15 = 34/15 ≈ 2.267 -> ~45.
    let score = client.get_reputation(&artist);
    assert!(score > 36, "weighted score {score} should exceed the plain mean 36");
}

#[test]
fn dispute_excludes_review_until_resolved() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);

    client.submit_review(&reviewer, &artist, &1, &String::from_str(&env, "unfair"));
    assert_eq!(client.get_reputation(&artist), 4); // 1-star (base 20) at 1/5 confidence -> 20*20% = 4

    client.dispute_review(&artist, &0, &String::from_str(&env, "this is false"));
    // Disputed review no longer counts -> no counted reviews -> score 0.
    assert_eq!(client.get_reputation(&artist), 0);

    let review = client.get_review(&artist, &0);
    assert_eq!(review.status, ReviewStatus::Disputed);

    // Admin acts as a moderator by default.
    client.resolve_dispute(&admin, &artist, &0, &false, &String::from_str(&env, "review removed"));
    let review = client.get_review(&artist, &0);
    assert_eq!(review.status, ReviewStatus::Removed);
    assert_eq!(client.get_reputation(&artist), 0);
}

#[test]
fn upheld_dispute_restores_the_review_to_scoring() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);

    client.submit_review(&reviewer, &artist, &3, &String::from_str(&env, "meh"));
    client.dispute_review(&artist, &0, &String::from_str(&env, "unfair"));
    client.resolve_dispute(&admin, &artist, &0, &true, &String::from_str(&env, "review stands"));

    let review = client.get_review(&artist, &0);
    assert_eq!(review.status, ReviewStatus::Upheld);
    assert_eq!(client.get_reputation(&artist), 12); // 3 * 20 / 5 = 12
}

#[test]
fn moderator_can_remove_a_review_directly() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);
    let moderator = Address::generate(&env);

    client.add_moderator(&moderator);
    assert!(client.is_moderator(&moderator));

    client.submit_review(&reviewer, &artist, &2, &String::from_str(&env, "spammy"));
    client.moderate_review(&moderator, &artist, &0, &String::from_str(&env, "spam"));

    let review = client.get_review(&artist, &0);
    assert_eq!(review.status, ReviewStatus::Removed);
    assert_eq!(client.get_reputation(&artist), 0);
    let _ = admin;
}

#[test]
fn non_moderator_cannot_resolve_disputes() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let artist = Address::generate(&env);
    let reviewer = Address::generate(&env);
    let stranger = Address::generate(&env);

    client.submit_review(&reviewer, &artist, &4, &String::from_str(&env, "good"));
    client.dispute_review(&artist, &0, &String::from_str(&env, "unfair"));

    let result = client.try_resolve_dispute(
        &stranger,
        &artist,
        &0,
        &true,
        &String::from_str(&env, "stands"),
    );
    assert_eq!(result, Err(Ok(ReputationError::Unauthorized)));
}

#[test]
fn artist_cannot_dispute_someone_elses_review() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let artist = Address::generate(&env);
    let other_artist = Address::generate(&env);
    let reviewer = Address::generate(&env);

    client.submit_review(&reviewer, &artist, &4, &String::from_str(&env, "good"));
    // other_artist tries to dispute a review filed against `artist`, indexed
    // under `artist`'s own review list — get_review(other_artist, 0) simply
    // won't exist, proving reviews are scoped per artist.
    let result = client.try_dispute_review(&other_artist, &0, &String::from_str(&env, "n/a"));
    assert_eq!(result, Err(Ok(ReputationError::ReviewNotFound)));
}

#[test]
fn unknown_artist_has_zero_reputation_and_no_reviews() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let artist = Address::generate(&env);

    assert_eq!(client.get_reputation(&artist), 0);
    assert_eq!(client.get_review_count(&artist), 0);
}
