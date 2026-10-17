//! Review Moderation & Appeal System
//!
//! Provides on-chain review management including:
//! - Review submission with rating and comment
//! - Review reporting (spam, abuse, misleading, other)
//! - Admin moderation queue and decision tracking
//! - Appeal mechanism for artists and clients
//! - Escalation to dispute arbiter
//!
//! Closes #604 – Add Review Moderation & Appeal System.

#![no_std]

pub mod errors;
pub mod score;
pub mod types;

#[cfg(test)]
mod tests;

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Bytes, Env, String};

use errors::ReputationError;
use types::{
    AppealRecord, AppealStatus, DataKey, ModerationRecord, ReportReason, ReportRecord,
    ReputationScore, ReviewAggregate, ReviewRecord, ReviewStatus, DEFAULT_APPEAL_WINDOW_LEDGERS,
    MAX_APPEAL_WINDOW_LEDGERS, MAX_QUEUE_SIZE, MAX_REPORTS_PER_REVIEW,
};

include!("../../semver_types.rs");

/// TTL for reputation data (~90 days at 6 s/ledger).
const REPUTATION_TTL_LEDGERS: u32 = 1_296_000;

/// Load a subject's maintained aggregate, or an all-zero one if they have never
/// been reviewed. Never returns `None`, so callers do not special-case absence
/// (#974).
fn load_aggregate(env: &Env, subject: &Address) -> ReviewAggregate {
    env.storage()
        .persistent()
        .get(&DataKey::Aggregate(subject.clone()))
        .unwrap_or(ReviewAggregate {
            subject: subject.clone(),
            total_reviews: 0,
            active_reviews: 0,
            rating_sum: 0,
            removed_reviews: 0,
            appeal_count: 0,
            moderation_count: 0,
        })
}

fn save_aggregate(env: &Env, aggregate: &ReviewAggregate) {
    let key = DataKey::Aggregate(aggregate.subject.clone());
    env.storage().persistent().set(&key, aggregate);
    env.storage()
        .persistent()
        .extend_ttl(&key, REPUTATION_TTL_LEDGERS, REPUTATION_TTL_LEDGERS);
}

/// Configured appeal window in ledgers, falling back to the default (#976).
fn configured_appeal_window(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::AppealWindowLedgers)
        .unwrap_or(DEFAULT_APPEAL_WINDOW_LEDGERS)
}

#[contract]
pub struct ReputationContract;

#[contractimpl]
impl ReputationContract {
    pub fn initialize(env: Env, admin: Address) -> Result<(), ReputationError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(ReputationError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.events().publish((symbol_short!("init"),), (admin,));
        Ok(())
    }

    impl_semver_queries!();

    pub fn submit_review(
        env: Env,
        review_id: Bytes,
        artist: Address,
        reviewer: Address,
        rating_x10: u32,
        comment: String,
    ) -> Result<(), ReputationError> {
        reviewer.require_auth();

        if rating_x10 < 10 || rating_x10 > 50 {
            return Err(ReputationError::InvalidRating);
        }
        if env.storage().persistent().has(&DataKey::Review(review_id.clone())) {
            return Err(ReputationError::AlreadyReported);
        }

        let record = ReviewRecord {
            review_id: review_id.clone(),
            artist: artist.clone(),
            reviewer: reviewer.clone(),
            rating_x10,
            comment,
            status: ReviewStatus::Active,
            created_ledger: env.ledger().sequence(),
        };
        env.storage().persistent().set(&DataKey::Review(review_id.clone()), &record);
        env.storage().persistent().extend_ttl(
            &DataKey::Review(review_id.clone()),
            REPUTATION_TTL_LEDGERS,
            REPUTATION_TTL_LEDGERS,
        );

        // #974: maintain the subject's rolling aggregate on write so reads are
        // O(1) and do not scan the review set.
        let mut aggregate = load_aggregate(&env, &artist);
        aggregate.total_reviews = aggregate.total_reviews.saturating_add(1);
        aggregate.active_reviews = aggregate.active_reviews.saturating_add(1);
        aggregate.rating_sum = aggregate.rating_sum.saturating_add(rating_x10);
        save_aggregate(&env, &aggregate);

        env.events().publish(
            (symbol_short!("review"),),
            (review_id, artist, reviewer, rating_x10),
        );
        Ok(())
    }

    pub fn get_review(env: Env, review_id: Bytes) -> Result<ReviewRecord, ReputationError> {
        env.storage()
            .persistent()
            .get(&DataKey::Review(review_id))
            .ok_or(ReputationError::ReviewNotFound)
    }

    pub fn report_review(
        env: Env,
        review_id: Bytes,
        reporter: Address,
        reason: ReportReason,
        details: String,
    ) -> Result<(), ReputationError> {
        reporter.require_auth();

        let mut review: ReviewRecord = env.storage()
            .persistent()
            .get(&DataKey::Review(review_id.clone()))
            .ok_or(ReputationError::ReviewNotFound)?;

        if review.status != ReviewStatus::Active && review.status != ReviewStatus::Cleared {
            return Err(ReputationError::InvalidStatus);
        }

        let count: u32 = env.storage()
            .persistent()
            .get(&DataKey::ReportCount(review_id.clone()))
            .unwrap_or(0u32);
        for i in 0..count {
            let r: ReportRecord = env.storage()
                .persistent()
                .get(&DataKey::Report(review_id.clone(), i))
                .unwrap();
            if r.reporter == reporter {
                return Err(ReputationError::AlreadyReported);
            }
        }

        // #976: bound the queue before writing. The queue is instance storage
        // that anyone can drive with `report_review`, so its growth is capped
        // both globally and per review.
        if count >= MAX_REPORTS_PER_REVIEW {
            return Err(ReputationError::ReportLimitReached);
        }
        let queue_size_precheck: u32 = env
            .storage()
            .instance()
            .get(&DataKey::QueueSize)
            .unwrap_or(0u32);
        if queue_size_precheck >= MAX_QUEUE_SIZE {
            return Err(ReputationError::QueueFull);
        }

        let report = ReportRecord {
            review_id: review_id.clone(),
            reporter: reporter.clone(),
            reason,
            details,
            created_ledger: env.ledger().sequence(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::Report(review_id.clone(), count), &report);
        env.storage().persistent().extend_ttl(
            &DataKey::Report(review_id.clone(), count),
            REPUTATION_TTL_LEDGERS,
            REPUTATION_TTL_LEDGERS,
        );
        env.storage()
            .persistent()
            .set(&DataKey::ReportCount(review_id.clone()), &(count + 1));
        env.storage().persistent().extend_ttl(
            &DataKey::ReportCount(review_id.clone()),
            REPUTATION_TTL_LEDGERS,
            REPUTATION_TTL_LEDGERS,
        );

        review.status = ReviewStatus::UnderReview;
        env.storage()
            .persistent()
            .set(&DataKey::Review(review_id.clone()), &review);

        let queue_size: u32 = env.storage()
            .instance()
            .get(&DataKey::QueueSize)
            .unwrap_or(0u32);
        env.storage()
            .instance()
            .set(&DataKey::QueueEntry(queue_size), &review_id);
        env.storage().instance().set(&DataKey::QueueSize, &(queue_size + 1));

        env.events().publish((symbol_short!("reported"),), (review_id, reporter));
        Ok(())
    }

    pub fn get_moderation_queue_size(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::QueueSize).unwrap_or(0u32)
    }

    pub fn get_queue_entry(env: Env, index: u32) -> Result<Bytes, ReputationError> {
        env.storage()
            .instance()
            .get(&DataKey::QueueEntry(index))
            .ok_or(ReputationError::ReviewNotFound)
    }

    pub fn moderate_review(
        env: Env,
        review_id: Bytes,
        new_status: ReviewStatus,
        notes: String,
    ) -> Result<(), ReputationError> {
        let admin = Self::require_admin(&env)?;
        admin.require_auth();

        let mut review: ReviewRecord = env.storage()
            .persistent()
            .get(&DataKey::Review(review_id.clone()))
            .ok_or(ReputationError::ReviewNotFound)?;

        if review.status != ReviewStatus::UnderReview {
            return Err(ReputationError::InvalidStatus);
        }
        if new_status != ReviewStatus::Removed && new_status != ReviewStatus::Cleared {
            return Err(ReputationError::InvalidStatus);
        }

        // Status is guaranteed `UnderReview` here, so the review is currently
        // active; removing it takes it out of the active average.
        review.status = new_status.clone();
        env.storage()
            .persistent()
            .set(&DataKey::Review(review_id.clone()), &review);

        // #974: moderation changes the score intentionally — removing a review
        // takes it out of the active average; clearing it leaves the rating in
        // place.
        let mut aggregate = load_aggregate(&env, &review.artist);
        aggregate.moderation_count = aggregate.moderation_count.saturating_add(1);
        if new_status == ReviewStatus::Removed {
            aggregate.active_reviews = aggregate.active_reviews.saturating_sub(1);
            aggregate.rating_sum = aggregate.rating_sum.saturating_sub(review.rating_x10);
            aggregate.removed_reviews = aggregate.removed_reviews.saturating_add(1);
        }
        save_aggregate(&env, &aggregate);

        let mod_count: u32 = env.storage()
            .persistent()
            .get(&DataKey::ModerationCount(review_id.clone()))
            .unwrap_or(0u32);
        let entry = ModerationRecord {
            review_id: review_id.clone(),
            admin: admin.clone(),
            new_status,
            notes,
            ledger: env.ledger().sequence(),
        };
        env.storage().persistent().set(
            &DataKey::ModerationEntry(review_id.clone(), mod_count),
            &entry,
        );
        env.storage().persistent().extend_ttl(
            &DataKey::ModerationEntry(review_id.clone(), mod_count),
            REPUTATION_TTL_LEDGERS,
            REPUTATION_TTL_LEDGERS,
        );
        env.storage().persistent().set(
            &DataKey::ModerationCount(review_id.clone()),
            &(mod_count + 1),
        );
        env.storage().persistent().extend_ttl(
            &DataKey::ModerationCount(review_id.clone()),
            REPUTATION_TTL_LEDGERS,
            REPUTATION_TTL_LEDGERS,
        );

        env.events().publish((symbol_short!("moderated"),), (review_id, admin));
        Ok(())
    }

    pub fn get_moderation_entry(
        env: Env,
        review_id: Bytes,
        index: u32,
    ) -> Result<ModerationRecord, ReputationError> {
        env.storage()
            .persistent()
            .get(&DataKey::ModerationEntry(review_id, index))
            .ok_or(ReputationError::ReviewNotFound)
    }

    pub fn get_moderation_count(env: Env, review_id: Bytes) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::ModerationCount(review_id))
            .unwrap_or(0u32)
    }

    pub fn file_appeal(
        env: Env,
        review_id: Bytes,
        appellant: Address,
        reason: String,
    ) -> Result<(), ReputationError> {
        appellant.require_auth();

        let review: ReviewRecord = env.storage()
            .persistent()
            .get(&DataKey::Review(review_id.clone()))
            .ok_or(ReputationError::ReviewNotFound)?;

        if review.status != ReviewStatus::Removed {
            return Err(ReputationError::InvalidStatus);
        }
        if appellant != review.artist && appellant != review.reviewer {
            return Err(ReputationError::Unauthorized);
        }
        if env.storage().persistent().has(&DataKey::Appeal(review_id.clone())) {
            return Err(ReputationError::AppealAlreadyExists);
        }

        // #976: filing is bounded by the review's appeal window. The deadline
        // ledger itself is still inside the window; only a chain that has moved
        // past it rejects the appeal.
        let deadline = review.created_ledger.saturating_add(configured_appeal_window(&env));
        if env.ledger().sequence() > deadline {
            return Err(ReputationError::AppealWindowClosed);
        }

        let appeal = AppealRecord {
            review_id: review_id.clone(),
            appellant: appellant.clone(),
            reason,
            status: AppealStatus::Pending,
            created_ledger: env.ledger().sequence(),
            window_deadline_ledger: deadline,
        };
        env.storage().persistent().set(&DataKey::Appeal(review_id.clone()), &appeal);
        env.storage().persistent().extend_ttl(
            &DataKey::Appeal(review_id.clone()),
            REPUTATION_TTL_LEDGERS,
            REPUTATION_TTL_LEDGERS,
        );

        let mut aggregate = load_aggregate(&env, &review.artist);
        aggregate.appeal_count = aggregate.appeal_count.saturating_add(1);
        save_aggregate(&env, &aggregate);

        env.events().publish((symbol_short!("appeal"),), (review_id, appellant));
        Ok(())
    }

    pub fn resolve_appeal(
        env: Env,
        review_id: Bytes,
        decision: AppealStatus,
    ) -> Result<(), ReputationError> {
        let admin = Self::require_admin(&env)?;
        admin.require_auth();

        let mut appeal: AppealRecord = env.storage()
            .persistent()
            .get(&DataKey::Appeal(review_id.clone()))
            .ok_or(ReputationError::AppealNotFound)?;

        if appeal.status != AppealStatus::Pending {
            return Err(ReputationError::InvalidStatus);
        }
        if decision == AppealStatus::Pending {
            return Err(ReputationError::InvalidStatus);
        }

        appeal.status = decision.clone();
        env.storage().persistent().set(&DataKey::Appeal(review_id.clone()), &appeal);

        if decision == AppealStatus::Upheld {
            let mut review: ReviewRecord = env.storage()
                .persistent()
                .get(&DataKey::Review(review_id.clone()))
                .ok_or(ReputationError::ReviewNotFound)?;
            let was_removed = review.status == ReviewStatus::Removed;
            review.status = ReviewStatus::Active;
            env.storage().persistent().set(&DataKey::Review(review_id.clone()), &review);

            // #974: a successful appeal reinstates a removed rating, so the
            // subject's aggregate moves back in step with the review.
            if was_removed {
                let mut aggregate = load_aggregate(&env, &review.artist);
                aggregate.active_reviews = aggregate.active_reviews.saturating_add(1);
                aggregate.rating_sum = aggregate.rating_sum.saturating_add(review.rating_x10);
                aggregate.removed_reviews = aggregate.removed_reviews.saturating_sub(1);
                save_aggregate(&env, &aggregate);
            }
        }

        env.events().publish((symbol_short!("appeal_rs"),), (review_id, admin));
        Ok(())
    }

    pub fn get_appeal(env: Env, review_id: Bytes) -> Result<AppealRecord, ReputationError> {
        env.storage()
            .persistent()
            .get(&DataKey::Appeal(review_id))
            .ok_or(ReputationError::AppealNotFound)
    }

    pub fn get_report_count(env: Env, review_id: Bytes) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::ReportCount(review_id))
            .unwrap_or(0u32)
    }

    pub fn get_report(env: Env, review_id: Bytes, index: u32) -> Result<ReportRecord, ReputationError> {
        env.storage()
            .persistent()
            .get(&DataKey::Report(review_id, index))
            .ok_or(ReputationError::ReviewNotFound)
    }

    pub fn escalate_appeal(
        env: Env,
        review_id: Bytes,
        appellant: Address,
    ) -> Result<(), ReputationError> {
        appellant.require_auth();

        let mut appeal: AppealRecord = env.storage()
            .persistent()
            .get(&DataKey::Appeal(review_id.clone()))
            .ok_or(ReputationError::AppealNotFound)?;

        if appeal.status != AppealStatus::Pending {
            return Err(ReputationError::InvalidStatus);
        }
        if appeal.appellant != appellant {
            return Err(ReputationError::Unauthorized);
        }

        appeal.status = AppealStatus::Escalated;
        env.storage().persistent().set(&DataKey::Appeal(review_id.clone()), &appeal);

        env.events().publish((symbol_short!("escalated"),), (review_id, appellant));
        Ok(())
    }

    // ── Reputation aggregate (#974) ──────────────────────────────────────────

    /// Aggregate reputation for `subject`, with its components exposed rather
    /// than only a total.
    ///
    /// Read-only and O(1): it reads one maintained entry and projects it (see
    /// `score.rs`), so a client never recomputes a score from the review set on
    /// its own. `search`'s `set_rating` is a private ranking hint; this contract
    /// is the authoritative reputation source.
    pub fn get_score(env: Env, subject: Address) -> ReputationScore {
        score::score_from_aggregate(&load_aggregate(&env, &subject))
    }

    /// The maintained aggregate behind `get_score`, for audit.
    pub fn get_aggregate(env: Env, subject: Address) -> ReviewAggregate {
        load_aggregate(&env, &subject)
    }

    // ── Appeal window (#976) ─────────────────────────────────────────────────

    /// Configured appeal window, in ledgers.
    pub fn get_appeal_window(env: Env) -> u32 {
        configured_appeal_window(&env)
    }

    /// Set the appeal window (admin only). Zero and values above
    /// [`MAX_APPEAL_WINDOW_LEDGERS`](crate::types::MAX_APPEAL_WINDOW_LEDGERS)
    /// are rejected rather than silently clamped.
    pub fn set_appeal_window(
        env: Env,
        admin: Address,
        ledgers: u32,
    ) -> Result<(), ReputationError> {
        let stored = Self::require_admin(&env)?;
        admin.require_auth();
        if stored != admin {
            return Err(ReputationError::Unauthorized);
        }
        if ledgers == 0 || ledgers > MAX_APPEAL_WINDOW_LEDGERS {
            return Err(ReputationError::InvalidAppealWindow);
        }
        env.storage()
            .instance()
            .set(&DataKey::AppealWindowLedgers, &ledgers);
        Ok(())
    }

    /// Ledger through which `review_id` may still be appealed.
    pub fn get_appeal_deadline(env: Env, review_id: Bytes) -> Result<u32, ReputationError> {
        let review: ReviewRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Review(review_id))
            .ok_or(ReputationError::ReviewNotFound)?;
        Ok(review
            .created_ledger
            .saturating_add(configured_appeal_window(&env)))
    }

    /// Ledgers left to file an appeal for `review_id`; `0` once the window has
    /// closed. Lets a client tell a user "you have N ledgers left" instead of
    /// surfacing the refusal at filing time (#976).
    pub fn ledgers_to_appeal(env: Env, review_id: Bytes) -> Result<u32, ReputationError> {
        let deadline = Self::get_appeal_deadline(env.clone(), review_id)?;
        Ok(deadline.saturating_sub(env.ledger().sequence()))
    }

    fn require_admin(env: &Env) -> Result<Address, ReputationError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ReputationError::NotInitialized)
    }
}
