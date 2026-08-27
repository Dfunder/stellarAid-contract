//! Reputation contract — artist ratings, reviews, and reputation scoring.
//!
//! Implements the reputation & rating system requested in #597:
//! - one review per (artist, client) pair (closes duplicate-review requirement)
//! - review moderation: artists can dispute a review, moderators resolve it or
//!   remove reviews directly
//! - a recency-weighted, confidence-adjusted reputation score

#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, String, Vec};

pub mod errors;
pub mod types;

#[cfg(test)]
mod test;

use errors::ReputationError;
use types::{DataKey, Review, ReviewStatus};

include!("../../semver_types.rs");

/// Maximum byte length for a review comment / dispute reason / moderation note.
const MAX_COMMENT_LEN: u32 = 512;

/// Number of counted reviews at which the confidence factor reaches 100%.
/// Below this, the score is scaled down proportionally so a single 5-star
/// review does not immediately read as a maximal reputation.
const MIN_REVIEWS_FOR_FULL_CONFIDENCE: u32 = 5;

#[contract]
pub struct Reputation;

fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

fn get_admin(env: &Env) -> Result<Address, ReputationError> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(ReputationError::NotInitialized)
}

fn require_admin(env: &Env) -> Result<Address, ReputationError> {
    let admin = get_admin(env)?;
    admin.require_auth();
    Ok(admin)
}

fn require_moderator(env: &Env, moderator: &Address) -> Result<(), ReputationError> {
    let admin = get_admin(env)?;
    let is_admin = admin == *moderator;
    if !is_admin
        && !env
            .storage()
            .instance()
            .has(&DataKey::Moderator(moderator.clone()))
    {
        return Err(ReputationError::Unauthorized);
    }
    moderator.require_auth();
    Ok(())
}

fn require_comment_len(comment: &String) -> Result<(), ReputationError> {
    if comment.len() > MAX_COMMENT_LEN {
        return Err(ReputationError::CommentTooLong);
    }
    Ok(())
}

fn load_reviews(env: &Env, artist: &Address) -> Vec<Review> {
    env.storage()
        .persistent()
        .get(&DataKey::ReviewsForArtist(artist.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

fn save_reviews(env: &Env, artist: &Address, reviews: &Vec<Review>) {
    env.storage()
        .persistent()
        .set(&DataKey::ReviewsForArtist(artist.clone()), reviews);
}

/// Recency-weighted, confidence-adjusted reputation score on a 0..=100 scale.
///
/// Two factors feed the score (closes "based on multiple factors"):
/// 1. A weighted average of counted ratings, where later (more recent)
///    reviews carry a larger weight than older ones.
/// 2. A confidence factor that scales the score down while the artist has
///    fewer than `MIN_REVIEWS_FOR_FULL_CONFIDENCE` counted reviews, so a
///    single perfect review does not read as a perfect reputation.
fn compute_reputation(reviews: &Vec<Review>) -> u32 {
    let mut weighted_sum: u64 = 0;
    let mut weight_total: u64 = 0;
    let mut counted: u32 = 0;

    for (i, review) in reviews.iter().enumerate() {
        if !review.status.counts_toward_score() {
            continue;
        }
        let weight = (i as u64) + 1;
        weighted_sum += (review.rating as u64) * weight;
        weight_total += weight;
        counted += 1;
    }

    if weight_total == 0 {
        return 0;
    }

    // Weighted average rating (1..=5) scaled to 0..=100.
    let base = (weighted_sum * 20) / weight_total;

    // Confidence factor in basis points, capped at 100%.
    let confidence_bps = core::cmp::min(
        10_000u64,
        (counted as u64) * (10_000u64 / MIN_REVIEWS_FOR_FULL_CONFIDENCE as u64),
    );

    ((base * confidence_bps) / 10_000) as u32
}

fn recompute_and_store(env: &Env, artist: &Address, reviews: &Vec<Review>) -> u32 {
    let score = compute_reputation(reviews);
    env.storage()
        .persistent()
        .set(&DataKey::ReputationScore(artist.clone()), &score);
    score
}

#[contractimpl]
impl Reputation {
    pub fn initialize(env: Env, admin: Address) -> Result<(), ReputationError> {
        if has_admin(&env) {
            return Err(ReputationError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.events().publish((symbol_short!("init"),), admin);
        Ok(())
    }

    impl_semver_queries!();

    pub fn add_moderator(env: Env, moderator: Address) -> Result<(), ReputationError> {
        require_admin(&env)?;
        env.storage()
            .instance()
            .set(&DataKey::Moderator(moderator.clone()), &true);
        env.events()
            .publish((symbol_short!("mod_add"),), moderator);
        Ok(())
    }

    pub fn remove_moderator(env: Env, moderator: Address) -> Result<(), ReputationError> {
        require_admin(&env)?;
        env.storage()
            .instance()
            .remove(&DataKey::Moderator(moderator.clone()));
        env.events().publish((symbol_short!("mod_rm"),), moderator);
        Ok(())
    }

    pub fn is_moderator(env: Env, moderator: Address) -> bool {
        env.storage()
            .instance()
            .has(&DataKey::Moderator(moderator))
    }

    /// Submit a rating + review for an artist. One review per (artist, client)
    /// pair — resubmission is rejected rather than silently overwriting, so a
    /// client cannot inflate or bury a score by repeating themselves.
    pub fn submit_review(
        env: Env,
        client: Address,
        artist: Address,
        rating: u32,
        comment: String,
    ) -> Result<u32, ReputationError> {
        if !has_admin(&env) {
            return Err(ReputationError::NotInitialized);
        }
        client.require_auth();

        if !(1..=5).contains(&rating) {
            return Err(ReputationError::InvalidRating);
        }
        require_comment_len(&comment)?;

        let dup_key = DataKey::HasReviewed(artist.clone(), client.clone());
        if env.storage().persistent().has(&dup_key) {
            return Err(ReputationError::DuplicateReview);
        }

        let mut reviews = load_reviews(&env, &artist);
        let index = reviews.len();
        let review = Review {
            client: client.clone(),
            artist: artist.clone(),
            rating,
            comment,
            status: ReviewStatus::Active,
            ledger: env.ledger().sequence(),
            dispute_reason: None,
            moderation_note: None,
        };
        reviews.push_back(review);
        save_reviews(&env, &artist, &reviews);
        env.storage().persistent().set(&dup_key, &true);

        let score = recompute_and_store(&env, &artist, &reviews);

        env.events().publish(
            (symbol_short!("rev_new"),),
            (artist, client, rating, index, score),
        );
        Ok(index)
    }

    /// The artist being reviewed disputes a review about them. Disputed
    /// reviews are excluded from the score until a moderator resolves them.
    pub fn dispute_review(
        env: Env,
        artist: Address,
        review_index: u32,
        reason: String,
    ) -> Result<(), ReputationError> {
        if !has_admin(&env) {
            return Err(ReputationError::NotInitialized);
        }
        artist.require_auth();
        require_comment_len(&reason)?;

        let mut reviews = load_reviews(&env, &artist);
        let mut review = reviews
            .get(review_index)
            .ok_or(ReputationError::ReviewNotFound)?;
        if review.artist != artist {
            return Err(ReputationError::Unauthorized);
        }
        if review.status != ReviewStatus::Active {
            return Err(ReputationError::InvalidStatus);
        }
        review.status = ReviewStatus::Disputed;
        review.dispute_reason = Some(reason);
        reviews.set(review_index, review);
        save_reviews(&env, &artist, &reviews);

        let score = recompute_and_store(&env, &artist, &reviews);

        env.events().publish(
            (symbol_short!("rev_disp"),),
            (artist, review_index, score),
        );
        Ok(())
    }

    /// Moderator resolves a disputed review: uphold (counts again) or remove
    /// (permanently excluded).
    pub fn resolve_dispute(
        env: Env,
        moderator: Address,
        artist: Address,
        review_index: u32,
        uphold_review: bool,
        note: String,
    ) -> Result<(), ReputationError> {
        require_moderator(&env, &moderator)?;
        require_comment_len(&note)?;

        let mut reviews = load_reviews(&env, &artist);
        let mut review = reviews
            .get(review_index)
            .ok_or(ReputationError::ReviewNotFound)?;
        if review.status != ReviewStatus::Disputed {
            return Err(ReputationError::InvalidStatus);
        }
        review.status = if uphold_review {
            ReviewStatus::Upheld
        } else {
            ReviewStatus::Removed
        };
        review.moderation_note = Some(note);
        reviews.set(review_index, review);
        save_reviews(&env, &artist, &reviews);

        let score = recompute_and_store(&env, &artist, &reviews);

        env.events().publish(
            (symbol_short!("rev_res"),),
            (artist, review_index, uphold_review, score),
        );
        Ok(())
    }

    /// Direct moderation of an active review (e.g. spam/abuse), without going
    /// through the artist-initiated dispute flow.
    pub fn moderate_review(
        env: Env,
        moderator: Address,
        artist: Address,
        review_index: u32,
        note: String,
    ) -> Result<(), ReputationError> {
        require_moderator(&env, &moderator)?;
        require_comment_len(&note)?;

        let mut reviews = load_reviews(&env, &artist);
        let mut review = reviews
            .get(review_index)
            .ok_or(ReputationError::ReviewNotFound)?;
        if review.status == ReviewStatus::Removed {
            return Err(ReputationError::InvalidStatus);
        }
        review.status = ReviewStatus::Removed;
        review.moderation_note = Some(note);
        reviews.set(review_index, review);
        save_reviews(&env, &artist, &reviews);

        let score = recompute_and_store(&env, &artist, &reviews);

        env.events()
            .publish((symbol_short!("rev_mod"),), (artist, review_index, score));
        Ok(())
    }

    pub fn get_reviews(env: Env, artist: Address) -> Vec<Review> {
        load_reviews(&env, &artist)
    }

    pub fn get_review(env: Env, artist: Address, review_index: u32) -> Result<Review, ReputationError> {
        load_reviews(&env, &artist)
            .get(review_index)
            .ok_or(ReputationError::ReviewNotFound)
    }

    pub fn has_reviewed(env: Env, artist: Address, client: Address) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::HasReviewed(artist, client))
    }

    /// Cached 0..=100 reputation score; 0 if the artist has no counted reviews.
    pub fn get_reputation(env: Env, artist: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::ReputationScore(artist))
            .unwrap_or(0)
    }

    pub fn get_review_count(env: Env, artist: Address) -> u32 {
        load_reviews(&env, &artist).len()
    }
}
