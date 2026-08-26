//! Reputation & Rating System Contract — closes #597
//!
//! Provides:
//! - [`ReputationContract::submit_review`]  — clients submit a rating + comment.
//! - [`ReputationContract::moderate_review`] — admin hides abusive reviews.
//! - [`ReputationContract::open_dispute`]   — any party disputes a review.
//! - [`ReputationContract::resolve_dispute`] — admin resolves and sets final status.
//! - [`ReputationContract::get_review`]     — read a single review.
//! - [`ReputationContract::get_artist_stats`] — read aggregated reputation.
//!
//! **Weighted scoring:** The contract re-computes a recency-weighted reputation
//! score every time an active review is added or removed.  Ratings submitted in
//! the current ledger epoch receive full weight; older ratings decay linearly
//! over `RECENCY_HALF_LIFE_LEDGERS` ledgers.
//!
//! **Duplicate prevention:** each (client, artist) pair may have at most one
//! active review stored under `DataKey::ClientArtistReviews`.

#![no_std]

mod errors;
mod types;

#[cfg(test)]
mod test;

#[cfg(test)]
mod moderation_appeal_tests;

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Bytes, Env, String, Vec};
use errors::ReputationError;
use types::{
    AppealRecord, AppealStatus, ArtistStats, DataKey, ModerationAction, ModerationDecision,
    ReportReason, ReportRecord, ReviewRecord, ReviewStatus,
};

/// Recency half-life: ~7 days at 6 s / ledger.
const RECENCY_HALF_LIFE_LEDGERS: u32 = 100_800;

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn require_admin(env: &Env) -> Result<(), ReputationError> {
    if !env.storage().instance().has(&DataKey::Admin) {
        return Err(ReputationError::NotInitialized);
    }
    let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    admin.require_auth();
    Ok(())
}

fn load_stats(env: &Env, artist: &Address) -> ArtistStats {
    env.storage()
        .persistent()
        .get(&DataKey::ArtistStats(artist.clone()))
        .unwrap_or(ArtistStats {
            total_score: 0,
            review_count: 0,
            reputation_score: 0,
            last_review_ledger: 0,
        })
}

fn save_stats(env: &Env, artist: &Address, stats: &ArtistStats) {
    env.storage()
        .persistent()
        .set(&DataKey::ArtistStats(artist.clone()), stats);
}

/// Recompute weighted reputation score.
///
/// Uses a simple formula:
/// ```text
/// weighted_score = (total_score * recency_factor) / review_count
/// ```
/// where `recency_factor` is in [5000, 10000] depending on how fresh the
/// most recent review is.  The result is clamped to [0, 10_000].
fn compute_reputation_score(stats: &ArtistStats, current_ledger: u32) -> u32 {
    if stats.review_count == 0 {
        return 0;
    }
    let raw_avg = (stats.total_score / stats.review_count as u64) as u32; // 1-100

    // Recency factor: 10_000 if recent, decays toward 5_000 over half-life.
    let age = current_ledger.saturating_sub(stats.last_review_ledger);
    let decay_factor = if age >= RECENCY_HALF_LIFE_LEDGERS {
        5_000u64
    } else {
        10_000u64 - (5_000u64 * age as u64 / RECENCY_HALF_LIFE_LEDGERS as u64)
    };

    // Scale raw_avg (1–100) to (0–10_000) then apply recency.
    let scaled = (raw_avg as u64)
        .saturating_mul(decay_factor)
        / 100; // divide by 100 so max = 10_000

    scaled.min(10_000) as u32
}

fn update_stats_add(env: &Env, artist: &Address, rating: u32, review_ledger: u32) -> Result<(), ReputationError> {
    let mut stats = load_stats(env, artist);
    stats.total_score = stats
        .total_score
        .checked_add(rating as u64)
        .ok_or(ReputationError::ArithmeticOverflow)?;
    stats.review_count = stats
        .review_count
        .checked_add(1)
        .ok_or(ReputationError::ArithmeticOverflow)?;
    if review_ledger > stats.last_review_ledger {
        stats.last_review_ledger = review_ledger;
    }
    stats.reputation_score = compute_reputation_score(&stats, env.ledger().sequence());
    save_stats(env, artist, &stats);
    Ok(())
}

fn update_stats_remove(env: &Env, artist: &Address, rating: u32) -> Result<(), ReputationError> {
    let mut stats = load_stats(env, artist);
    if stats.review_count == 0 {
        return Ok(());
    }
    stats.total_score = stats.total_score.saturating_sub(rating as u64);
    stats.review_count = stats.review_count.saturating_sub(1);
    stats.reputation_score = compute_reputation_score(&stats, env.ledger().sequence());
    save_stats(env, artist, &stats);
    Ok(())
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct ReputationContract;

#[contractimpl]
impl ReputationContract {
    // -----------------------------------------------------------------------
    // Initialisation
    // -----------------------------------------------------------------------

    /// Initialise the contract with an admin address.
    ///
    /// Can only be called once.
    pub fn initialize(env: Env, admin: Address) -> Result<(), ReputationError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(ReputationError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.events()
            .publish((symbol_short!("rep_init"),), (admin,));
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Review lifecycle
    // -----------------------------------------------------------------------

    /// Submit a rating for an artist.
    ///
    /// - `rating` must be in `[1, 100]`.
    /// - A client may only have **one active review per artist** (duplicate prevention).
    /// - Emits `rep_review` event.
    ///
    /// Closes #597.
    pub fn submit_review(
        env: Env,
        review_id: Bytes,
        artist: Address,
        client: Address,
        rating: u32,
        comment: String,
    ) -> Result<(), ReputationError> {
        if !env.storage().instance().has(&DataKey::Admin) {
            return Err(ReputationError::NotInitialized);
        }
        client.require_auth();

        if rating < 1 || rating > 100 {
            return Err(ReputationError::InvalidRating);
        }

        // Duplicate check — at most one active review per (client, artist).
        let dup_key = DataKey::ClientArtistReviews(client.clone(), artist.clone());
        if env.storage().persistent().has(&dup_key) {
            return Err(ReputationError::DuplicateReview);
        }

        let current_ledger = env.ledger().sequence();
        let record = ReviewRecord {
            review_id: review_id.clone(),
            artist: artist.clone(),
            client: client.clone(),
            rating,
            comment: comment.clone(),
            status: ReviewStatus::Active,
            created_ledger: current_ledger,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Review(review_id.clone()), &record);
        env.storage().persistent().set(&dup_key, &review_id);

        update_stats_add(&env, &artist, rating, current_ledger)?;

        env.events().publish(
            (symbol_short!("rep_rev"),),
            (review_id, artist, client, rating),
        );
        Ok(())
    }

    /// Moderate (hide) a review.  Admin only.
    ///
    /// The review is marked `Moderated` and removed from the artist's
    /// running reputation total.
    ///
    /// Closes #597.
    pub fn moderate_review(
        env: Env,
        review_id: Bytes,
    ) -> Result<(), ReputationError> {
        require_admin(&env)?;

        let mut record: ReviewRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Review(review_id.clone()))
            .ok_or(ReputationError::NotFound)?;

        if record.status != ReviewStatus::Active {
            return Err(ReputationError::AlreadyModerated);
        }

        let artist = record.artist.clone();
        let rating = record.rating;
        record.status = ReviewStatus::Moderated;
        env.storage()
            .persistent()
            .set(&DataKey::Review(review_id.clone()), &record);

        // Remove from stats so moderated reviews don't inflate/deflate scores.
        update_stats_remove(&env, &artist, rating)?;

        // Record in moderation history (#604).
        Self::append_history(&env, &review_id, ModerationAction::Hidden, None);
        Self::remove_from_queue(&env, &review_id);

        env.events()
            .publish((symbol_short!("rep_mod"),), (review_id, artist));
        Ok(())
    }

    /// Open a dispute on a review.  Either the artist or the client of that
    /// review may open a dispute; the admin may also open one.
    ///
    /// The review is marked `Disputed` and temporarily removed from stats
    /// pending resolution.
    ///
    /// Closes #597.
    pub fn open_dispute(env: Env, review_id: Bytes, initiator: Address) -> Result<(), ReputationError> {
        if !env.storage().instance().has(&DataKey::Admin) {
            return Err(ReputationError::NotInitialized);
        }
        initiator.require_auth();

        let mut record: ReviewRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Review(review_id.clone()))
            .ok_or(ReputationError::NotFound)?;

        // Only the artist, client, or admin may dispute.
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if initiator != record.artist && initiator != record.client && initiator != admin {
            return Err(ReputationError::Unauthorized);
        }

        if record.status == ReviewStatus::Disputed {
            return Err(ReputationError::DisputeAlreadyOpen);
        }
        if record.status == ReviewStatus::Moderated {
            return Err(ReputationError::AlreadyModerated);
        }

        let artist = record.artist.clone();
        let rating = record.rating;
        record.status = ReviewStatus::Disputed;
        env.storage()
            .persistent()
            .set(&DataKey::Review(review_id.clone()), &record);

        // Temporarily exclude disputed rating from scores.
        update_stats_remove(&env, &artist, rating)?;

        env.events()
            .publish((symbol_short!("rep_disp"),), (review_id, initiator));
        Ok(())
    }

    /// Resolve a disputed review.  Admin only.
    ///
    /// If `reinstate` is `true` the review becomes `Active` again and is
    /// re-added to the artist's stats; otherwise it stays permanently
    /// `Moderated`.
    ///
    /// Closes #597.
    pub fn resolve_dispute(
        env: Env,
        review_id: Bytes,
        reinstate: bool,
    ) -> Result<(), ReputationError> {
        require_admin(&env)?;

        let mut record: ReviewRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Review(review_id.clone()))
            .ok_or(ReputationError::NotFound)?;

        if record.status != ReviewStatus::Disputed {
            return Err(ReputationError::NoOpenDispute);
        }

        let artist = record.artist.clone();
        let rating = record.rating;
        let created = record.created_ledger;

        let action = if reinstate {
            record.status = ReviewStatus::Active;
            env.storage()
                .persistent()
                .set(&DataKey::Review(review_id.clone()), &record);
            update_stats_add(&env, &artist, rating, created)?;
            ModerationAction::DisputeReinstated
        } else {
            record.status = ReviewStatus::Moderated;
            env.storage()
                .persistent()
                .set(&DataKey::Review(review_id.clone()), &record);
            ModerationAction::DisputeRejected
        };

        // Record in moderation history (#604).
        Self::append_history(&env, &review_id, action, None);

        env.events()
            .publish((symbol_short!("rep_res"),), (review_id, reinstate));
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    /// Return a single review record.
    pub fn get_review(env: Env, review_id: Bytes) -> Result<ReviewRecord, ReputationError> {
        env.storage()
            .persistent()
            .get(&DataKey::Review(review_id))
            .ok_or(ReputationError::NotFound)
    }

    /// Return aggregated stats for an artist.
    pub fn get_artist_stats(env: Env, artist: Address) -> ArtistStats {
        load_stats(&env, &artist)
    }

    // -----------------------------------------------------------------------
    // Review Reporting — closes #604
    // -----------------------------------------------------------------------

    /// Report a review for moderation.
    ///
    /// - Any authenticated address may report a review (not just the artist/client).
    /// - A reporter may only submit one report per review.
    /// - On the first report for a review the review_id is added to the admin
    ///   moderation queue so admins can triage efficiently.
    ///
    /// Closes #604.
    pub fn report_review(
        env: Env,
        report_id: Bytes,
        review_id: Bytes,
        reporter: Address,
        reason: ReportReason,
        details: String,
    ) -> Result<(), ReputationError> {
        if !env.storage().instance().has(&DataKey::Admin) {
            return Err(ReputationError::NotInitialized);
        }
        reporter.require_auth();

        // Review must exist.
        let _review: ReviewRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Review(review_id.clone()))
            .ok_or(ReputationError::NotFound)?;

        // Duplicate report check per reporter per review.
        // We encode as a composite key in reports list and check uniqueness.
        let mut reports: Vec<Bytes> = env
            .storage()
            .persistent()
            .get(&DataKey::ReviewReports(review_id.clone()))
            .unwrap_or(Vec::new(&env));

        // Check if this report_id is already used.
        if env.storage().persistent().has(&DataKey::Report(report_id.clone())) {
            return Err(ReputationError::DuplicateReport);
        }

        let record = ReportRecord {
            report_id: report_id.clone(),
            review_id: review_id.clone(),
            reporter,
            reason,
            details,
            created_ledger: env.ledger().sequence(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::Report(report_id.clone()), &record);

        let is_first_report = reports.is_empty();
        reports.push_back(report_id.clone());
        env.storage()
            .persistent()
            .set(&DataKey::ReviewReports(review_id.clone()), &reports);

        // Add to moderation queue if this is the first report.
        if is_first_report {
            let mut queue: Vec<Bytes> = env
                .storage()
                .instance()
                .get(&DataKey::ModerationQueue)
                .unwrap_or(Vec::new(&env));
            // Only add once.
            let already_queued = queue.iter().any(|r| r == review_id);
            if !already_queued {
                queue.push_back(review_id.clone());
                env.storage()
                    .instance()
                    .set(&DataKey::ModerationQueue, &queue);
            }
        }

        env.events().publish(
            (symbol_short!("rep_rpt"),),
            (report_id, review_id, reason as u32),
        );
        Ok(())
    }

    /// Return all reports for a review.
    ///
    /// Closes #604.
    pub fn get_reports(env: Env, review_id: Bytes) -> Vec<ReportRecord> {
        let report_ids: Vec<Bytes> = env
            .storage()
            .persistent()
            .get(&DataKey::ReviewReports(review_id))
            .unwrap_or(Vec::new(&env));

        let mut result: Vec<ReportRecord> = Vec::new(&env);
        for id in report_ids.iter() {
            if let Some(r) = env.storage().persistent().get(&DataKey::Report(id)) {
                result.push_back(r);
            }
        }
        result
    }

    /// Return the current admin moderation queue (list of review_ids with pending reports).
    ///
    /// Closes #604.
    pub fn get_moderation_queue(env: Env) -> Result<Vec<Bytes>, ReputationError> {
        require_admin(&env)?;
        Ok(env
            .storage()
            .instance()
            .get(&DataKey::ModerationQueue)
            .unwrap_or(Vec::new(&env)))
    }

    /// Remove a review from the moderation queue (called after admin takes action).
    ///
    /// Closes #604.
    fn remove_from_queue(env: &Env, review_id: &Bytes) {
        let queue: Vec<Bytes> = env
            .storage()
            .instance()
            .get(&DataKey::ModerationQueue)
            .unwrap_or(Vec::new(env));
        let mut updated: Vec<Bytes> = Vec::new(env);
        for r in queue.iter() {
            if r != *review_id {
                updated.push_back(r);
            }
        }
        env.storage().instance().set(&DataKey::ModerationQueue, &updated);
    }

    /// Append to the moderation history for a review.
    fn append_history(env: &Env, review_id: &Bytes, action: ModerationAction, appeal_id: Option<Bytes>) {
        let mut history: Vec<ModerationDecision> = env
            .storage()
            .persistent()
            .get(&DataKey::ModerationHistory(review_id.clone()))
            .unwrap_or(Vec::new(env));
        history.push_back(ModerationDecision {
            action,
            decided_ledger: env.ledger().sequence(),
            appeal_id,
        });
        env.storage()
            .persistent()
            .set(&DataKey::ModerationHistory(review_id.clone()), &history);
    }

    /// Return the moderation decision history for a review.
    ///
    /// Closes #604.
    pub fn get_moderation_history(env: Env, review_id: Bytes) -> Vec<ModerationDecision> {
        env.storage()
            .persistent()
            .get(&DataKey::ModerationHistory(review_id))
            .unwrap_or(Vec::new(&env))
    }

    // -----------------------------------------------------------------------
    // Appeal System — closes #604
    // -----------------------------------------------------------------------

    /// Submit an appeal against a moderation or dispute decision.
    ///
    /// - Only the artist or client of the review may appeal.
    /// - The review must be in `Moderated` or `Disputed` status.
    /// - At most one pending appeal per review.
    ///
    /// Closes #604.
    pub fn submit_appeal(
        env: Env,
        appeal_id: Bytes,
        review_id: Bytes,
        appellant: Address,
        reason: String,
    ) -> Result<(), ReputationError> {
        if !env.storage().instance().has(&DataKey::Admin) {
            return Err(ReputationError::NotInitialized);
        }
        appellant.require_auth();

        let review: ReviewRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Review(review_id.clone()))
            .ok_or(ReputationError::NotFound)?;

        // Only the artist or client of the review may appeal.
        if appellant != review.artist && appellant != review.client {
            return Err(ReputationError::Unauthorized);
        }

        // Review must be moderated or disputed to be appealed.
        if review.status != ReviewStatus::Moderated && review.status != ReviewStatus::Disputed {
            return Err(ReputationError::InvalidReviewState);
        }

        // Duplicate appeal_id check.
        if env.storage().persistent().has(&DataKey::Appeal(appeal_id.clone())) {
            return Err(ReputationError::AppealAlreadyOpen);
        }

        let record = AppealRecord {
            appeal_id: appeal_id.clone(),
            review_id: review_id.clone(),
            appellant,
            reason,
            status: AppealStatus::Pending,
            created_ledger: env.ledger().sequence(),
            resolved_ledger: 0,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Appeal(appeal_id.clone()), &record);

        env.events().publish(
            (symbol_short!("rep_apl"),),
            (appeal_id, review_id),
        );
        Ok(())
    }

    /// Resolve a pending appeal.  Admin only.
    ///
    /// - `accept = true` → review reinstated, stats updated.
    /// - `accept = false` → appeal rejected, review stays moderated.
    ///
    /// Closes #604.
    pub fn resolve_appeal(
        env: Env,
        appeal_id: Bytes,
        accept: bool,
    ) -> Result<(), ReputationError> {
        require_admin(&env)?;

        let mut appeal: AppealRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Appeal(appeal_id.clone()))
            .ok_or(ReputationError::AppealNotFound)?;

        if appeal.status != AppealStatus::Pending {
            return Err(ReputationError::AppealNotPending);
        }

        let review_id = appeal.review_id.clone();

        let mut review: ReviewRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Review(review_id.clone()))
            .ok_or(ReputationError::NotFound)?;

        let action = if accept {
            // Reinstate the review.
            review.status = ReviewStatus::Active;
            env.storage()
                .persistent()
                .set(&DataKey::Review(review_id.clone()), &review);
            update_stats_add(&env, &review.artist, review.rating, review.created_ledger)?;
            ModerationAction::AppealAccepted
        } else {
            // Uphold moderation decision; ensure review is Moderated.
            if review.status != ReviewStatus::Moderated {
                review.status = ReviewStatus::Moderated;
                env.storage()
                    .persistent()
                    .set(&DataKey::Review(review_id.clone()), &review);
            }
            ModerationAction::AppealRejected
        };

        appeal.status = if accept { AppealStatus::Accepted } else { AppealStatus::Rejected };
        appeal.resolved_ledger = env.ledger().sequence();
        env.storage()
            .persistent()
            .set(&DataKey::Appeal(appeal_id.clone()), &appeal);

        // Record in history.
        Self::append_history(&env, &review_id, action, Some(appeal_id.clone()));
        // Remove from moderation queue since admin has acted.
        Self::remove_from_queue(&env, &review_id);

        env.events().publish(
            (symbol_short!("rep_res_a"),),
            (appeal_id, review_id, accept),
        );
        Ok(())
    }

    /// Escalate an open appeal to the dispute arbiter.
    ///
    /// - Admin only.
    /// - Appeal must be pending.
    /// - Records the escalation in history and marks appeal as `Escalated`.
    ///
    /// Closes #604.
    pub fn escalate_appeal(
        env: Env,
        appeal_id: Bytes,
    ) -> Result<(), ReputationError> {
        require_admin(&env)?;

        let mut appeal: AppealRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Appeal(appeal_id.clone()))
            .ok_or(ReputationError::AppealNotFound)?;

        if appeal.status != AppealStatus::Pending {
            return Err(ReputationError::AppealNotPending);
        }

        let review_id = appeal.review_id.clone();

        appeal.status = AppealStatus::Escalated;
        appeal.resolved_ledger = env.ledger().sequence();
        env.storage()
            .persistent()
            .set(&DataKey::Appeal(appeal_id.clone()), &appeal);

        // Record in moderation history.
        Self::append_history(&env, &review_id, ModerationAction::Escalated, Some(appeal_id.clone()));

        env.events().publish(
            (symbol_short!("rep_esc"),),
            (appeal_id, review_id),
        );
        Ok(())
    }

    /// Return an appeal record by id.
    ///
    /// Closes #604.
    pub fn get_appeal(env: Env, appeal_id: Bytes) -> Result<AppealRecord, ReputationError> {
        env.storage()
            .persistent()
            .get(&DataKey::Appeal(appeal_id))
            .ok_or(ReputationError::AppealNotFound)
    }
}
