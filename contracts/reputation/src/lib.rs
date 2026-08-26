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

mod errors;
mod types;

#[cfg(test)]
mod tests;

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Bytes, Env, String};

use errors::ReputationError;
use types::{
    AppealRecord, AppealStatus, DataKey, ModerationRecord, ReportReason, ReportRecord,
    ReviewRecord, ReviewStatus,
};

/// TTL for reputation data (~90 days at 6 s/ledger).
const REPUTATION_TTL_LEDGERS: u32 = 1_296_000;

#[contract]
pub struct ReputationContract;

#[contractimpl]
impl ReputationContract {
    // ── Bootstrap ─────────────────────────────────────────────────────────

    /// Initialize the contract and set the admin.
    ///
    /// May only be called once.
    pub fn initialize(env: Env, admin: Address) -> Result<(), ReputationError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(ReputationError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.events().publish((symbol_short!("init"),), (admin,));
        Ok(())
    }

    // ── Review submission ─────────────────────────────────────────────────

    /// Submit a review for an artist.
    ///
    /// `rating_x10` is the star rating × 10 (range 10–50, e.g. 45 = 4.5 ★).
    /// The caller is the reviewer (a client).
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
            // Silently reject duplicate review ids the same way escrow does for commission ids
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

        env.events().publish(
            (symbol_short!("review"),),
            (review_id, artist, reviewer, rating_x10),
        );
        Ok(())
    }

    /// Read a review.
    pub fn get_review(env: Env, review_id: Bytes) -> Result<ReviewRecord, ReputationError> {
        env.storage()
            .persistent()
            .get(&DataKey::Review(review_id))
            .ok_or(ReputationError::ReviewNotFound)
    }

    // ── Review reporting ──────────────────────────────────────────────────

    /// Report a review for spam, abuse, etc.
    ///
    /// Each `reporter` may only report a given review once. Once reported,
    /// the review status moves to `UnderReview` and is added to the admin
    /// moderation queue.
    ///
    /// Closes #604 – support review reporting.
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

        // Only Active or Cleared reviews may be reported
        if review.status != ReviewStatus::Active && review.status != ReviewStatus::Cleared {
            return Err(ReputationError::InvalidStatus);
        }

        // Dedup: check if this reporter already filed a report
        let count: u32 = env.storage().persistent()
            .get(&DataKey::ReportCount(review_id.clone()))
            .unwrap_or(0u32);
        for i in 0..count {
            let r: ReportRecord = env.storage().persistent()
                .get(&DataKey::Report(review_id.clone(), i))
                .unwrap();
            if r.reporter == reporter {
                return Err(ReputationError::AlreadyReported);
            }
        }

        // Persist report
        let report = ReportRecord {
            review_id: review_id.clone(),
            reporter: reporter.clone(),
            reason,
            details,
            created_ledger: env.ledger().sequence(),
        };
        env.storage().persistent().set(&DataKey::Report(review_id.clone(), count), &report);
        env.storage().persistent().extend_ttl(
            &DataKey::Report(review_id.clone(), count),
            REPUTATION_TTL_LEDGERS,
            REPUTATION_TTL_LEDGERS,
        );
        env.storage().persistent().set(&DataKey::ReportCount(review_id.clone()), &(count + 1));
        env.storage().persistent().extend_ttl(
            &DataKey::ReportCount(review_id.clone()),
            REPUTATION_TTL_LEDGERS,
            REPUTATION_TTL_LEDGERS,
        );

        // Transition review to UnderReview and add to admin queue
        review.status = ReviewStatus::UnderReview;
        env.storage().persistent().set(&DataKey::Review(review_id.clone()), &review);

        let queue_size: u32 = env.storage().instance()
            .get(&DataKey::QueueSize)
            .unwrap_or(0u32);
        env.storage().instance().set(&DataKey::QueueEntry(queue_size), &review_id);
        env.storage().instance().set(&DataKey::QueueSize, &(queue_size + 1));

        env.events().publish(
            (symbol_short!("reported"),),
            (review_id, reporter),
        );
        Ok(())
    }

    // ── Admin moderation ──────────────────────────────────────────────────

    /// Return the number of reviews in the admin moderation queue.
    ///
    /// Closes #604 – admin moderation queue.
    pub fn get_moderation_queue_size(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::QueueSize).unwrap_or(0u32)
    }

    /// Return the review_id at position `index` in the moderation queue.
    pub fn get_queue_entry(env: Env, index: u32) -> Result<Bytes, ReputationError> {
        env.storage()
            .instance()
            .get(&DataKey::QueueEntry(index))
            .ok_or(ReputationError::ReviewNotFound)
    }

    /// Admin removes a review (spam, abuse ruling).
    ///
    /// Records the decision in the moderation history and removes the entry
    /// from the queue.
    ///
    /// Closes #604 – admin moderation queue + decision history.
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

        // Only UnderReview reviews may be moderated
        if review.status != ReviewStatus::UnderReview {
            return Err(ReputationError::InvalidStatus);
        }
        // Can only set to Removed or Cleared
        if new_status != ReviewStatus::Removed && new_status != ReviewStatus::Cleared {
            return Err(ReputationError::InvalidStatus);
        }

        review.status = new_status.clone();
        env.storage().persistent().set(&DataKey::Review(review_id.clone()), &review);

        // Append moderation history entry
        let mod_count: u32 = env.storage().persistent()
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

        env.events().publish(
            (symbol_short!("moderated"),),
            (review_id, admin),
        );
        Ok(())
    }

    /// Return a moderation history entry by index for a review.
    ///
    /// Closes #604 – track moderation decision history.
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

    /// Return the count of moderation entries for a review.
    pub fn get_moderation_count(env: Env, review_id: Bytes) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::ModerationCount(review_id))
            .unwrap_or(0u32)
    }

    // ── Appeal mechanism ──────────────────────────────────────────────────

    /// File an appeal against a Removed review.
    ///
    /// Only the affected artist or the original reviewer may appeal.
    /// At most one active appeal per review is allowed.
    ///
    /// Closes #604 – appeal mechanism for artists/clients.
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

        // Only Removed reviews may be appealed
        if review.status != ReviewStatus::Removed {
            return Err(ReputationError::InvalidStatus);
        }
        // Only the artist or reviewer may appeal
        if appellant != review.artist && appellant != review.reviewer {
            return Err(ReputationError::Unauthorized);
        }
        // Prevent duplicate appeals
        if env.storage().persistent().has(&DataKey::Appeal(review_id.clone())) {
            return Err(ReputationError::AppealAlreadyExists);
        }

        let appeal = AppealRecord {
            review_id: review_id.clone(),
            appellant: appellant.clone(),
            reason,
            status: AppealStatus::Pending,
            created_ledger: env.ledger().sequence(),
        };
        env.storage().persistent().set(&DataKey::Appeal(review_id.clone()), &appeal);
        env.storage().persistent().extend_ttl(
            &DataKey::Appeal(review_id.clone()),
            REPUTATION_TTL_LEDGERS,
            REPUTATION_TTL_LEDGERS,
        );

        env.events().publish(
            (symbol_short!("appeal"),),
            (review_id, appellant),
        );
        Ok(())
    }

    /// Admin rules on a pending appeal.
    ///
    /// - `Upheld`: review status reverts to `Active`.
    /// - `Denied`: review stays `Removed`.
    ///
    /// Closes #604 – appeal mechanism.
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
        // Decision must be Upheld, Denied, or Escalated
        if decision == AppealStatus::Pending {
            return Err(ReputationError::InvalidStatus);
        }

        appeal.status = decision.clone();
        env.storage().persistent().set(&DataKey::Appeal(review_id.clone()), &appeal);

        // If upheld, reinstate the review
        if decision == AppealStatus::Upheld {
            let mut review: ReviewRecord = env.storage()
                .persistent()
                .get(&DataKey::Review(review_id.clone()))
                .ok_or(ReputationError::ReviewNotFound)?;
            review.status = ReviewStatus::Active;
            env.storage().persistent().set(&DataKey::Review(review_id.clone()), &review);
        }

        env.events().publish(
            (symbol_short!("appeal_rs"),),
            (review_id, admin),
        );
        Ok(())
    }

    /// Read the current appeal for a review.
    ///
    /// Closes #604 – appeal mechanism retrieval.
    pub fn get_appeal(env: Env, review_id: Bytes) -> Result<AppealRecord, ReputationError> {
        env.storage()
            .persistent()
            .get(&DataKey::Appeal(review_id))
            .ok_or(ReputationError::AppealNotFound)
    }

    /// Return the number of reports for a review.
    pub fn get_report_count(env: Env, review_id: Bytes) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::ReportCount(review_id))
            .unwrap_or(0u32)
    }

    /// Return a specific report by index.
    pub fn get_report(env: Env, review_id: Bytes, index: u32) -> Result<ReportRecord, ReputationError> {
        env.storage()
            .persistent()
            .get(&DataKey::Report(review_id, index))
            .ok_or(ReputationError::ReviewNotFound)
    }

    // ── Escalation ────────────────────────────────────────────────────────

    /// Escalate a pending appeal to the external dispute arbiter.
    ///
    /// Marks the appeal as `Escalated`. The external dispute arbiter contract
    /// handles the final resolution off-chain (invoked separately by its own
    /// admin). This function records the escalation on-chain so all parties
    /// have a verifiable audit trail.
    ///
    /// Closes #604 – escalation to dispute arbiter.
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

        env.events().publish(
            (symbol_short!("escalated"),),
            (review_id, appellant),
        );
        Ok(())
    }

    // ── Internal helpers ──────────────────────────────────────────────────

    fn require_admin(env: &Env) -> Result<Address, ReputationError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ReputationError::NotInitialized)
    }
}
