use soroban_sdk::{contracttype, Address, Bytes, String};

// ── Appeal-window and moderation-queue policy (#976) ───────────────────────
//
// Both bounds are expressed in ledgers, not timestamps, matching the rest of
// the workspace (`escrow`'s dispute TTL, `shared::pause`'s recovery delay).

/// Default appeal window: ~1 day at 5 s/ledger. An appeal must be filed within
/// this many ledgers of the review it targets.
pub const DEFAULT_APPEAL_WINDOW_LEDGERS: u32 = 17_280;
/// Longest appeal window an admin may configure: ~30 days at 5 s/ledger.
pub const MAX_APPEAL_WINDOW_LEDGERS: u32 = 518_400;
/// Maximum number of reviews that may sit in the moderation queue at once.
///
/// The queue is a `count` + `index` pair in instance storage; an unbounded
/// queue is unbounded state growth that anyone can drive with `report_review`.
/// Once full, new reports are rejected with `ReputationError::QueueFull` until
/// moderation retires entries.
pub const MAX_QUEUE_SIZE: u32 = 256;
/// Maximum reports a single review may accumulate.
///
/// A per-review cap keeps one review from occupying the whole queue. As with
/// the global cap, it is a deliberate policy choice, not an accident.
pub const MAX_REPORTS_PER_REVIEW: u32 = 16;

/// The current state of a review in the moderation system.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewStatus {
    Active = 0,
    UnderReview = 1,
    Removed = 2,
    Cleared = 3,
}

/// Reason category supplied when reporting a review.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportReason {
    Spam = 0,
    Abuse = 1,
    Misleading = 2,
    Other = 3,
}

/// Current state of an appeal filed against a moderation decision.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppealStatus {
    Pending = 0,
    Upheld = 1,
    Denied = 2,
    Escalated = 3,
}

/// A review submitted by a client about an artist.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewRecord {
    pub review_id: Bytes,
    pub artist: Address,
    pub reviewer: Address,
    pub rating_x10: u32,
    pub comment: String,
    pub status: ReviewStatus,
    pub created_ledger: u32,
}

/// A moderation report filed against a review.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReportRecord {
    pub review_id: Bytes,
    pub reporter: Address,
    pub reason: ReportReason,
    pub details: String,
    pub created_ledger: u32,
}

/// An appeal record filed by the artist or reviewer.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppealRecord {
    pub review_id: Bytes,
    pub appellant: Address,
    pub reason: String,
    pub status: AppealStatus,
    pub created_ledger: u32,
    /// Ledger through which the appeal could have been filed. Stored on the
    /// record so a client can show "you had until ledger N" without recomputing
    /// the window from the review (#976).
    pub window_deadline_ledger: u32,
}

/// A moderation action taken by the admin on a review.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModerationRecord {
    pub review_id: Bytes,
    pub admin: Address,
    pub new_status: ReviewStatus,
    pub notes: String,
    pub ledger: u32,
}

/// Rolling per-subject aggregate maintained on every review lifecycle change
/// (#974).
///
/// The aggregate is *maintained on write* rather than recomputed by scanning
/// reviews, so `get_score` is O(1) and its cost does not grow with the number
/// of reviews ever submitted. The taxonomy lives in `search`; this crate is the
/// authoritative reputation source (see the crate docs).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewAggregate {
    pub subject: Address,
    /// Every review ever submitted for the subject.
    pub total_reviews: u32,
    /// Reviews still counted toward the score — every review that is not
    /// `Removed`.
    pub active_reviews: u32,
    /// Sum of `rating_x10` over `active_reviews`.
    pub rating_sum: u32,
    /// Reviews currently `Removed` by moderation.
    pub removed_reviews: u32,
    /// Appeals ever filed for the subject's reviews.
    pub appeal_count: u32,
    /// Moderation actions ever taken on the subject's reviews.
    pub moderation_count: u32,
}

/// The client-facing reputation aggregate, exposing its components rather than
/// only a total so a caller can display, audit, or re-weight the pieces (#974).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationScore {
    pub subject: Address,
    pub total_reviews: u32,
    /// Reviews that currently count toward the score.
    pub active_reviews: u32,
    /// Mean rating of active reviews, in tenths; `0` when none are active.
    pub average_rating_x10: u32,
    /// Raw rating sum over active reviews, for audit.
    pub rating_sum: u32,
    /// Confidence-weighted rating in tenths (blended toward a neutral prior so
    /// a single review does not read as a perfect record). See `score.rs`.
    pub weighted_score_x10: u32,
    pub removed_reviews: u32,
    pub appeal_count: u32,
    pub moderation_count: u32,
}

/// Storage data keys.
#[contracttype]
pub enum DataKey {
    Admin,
    Review(Bytes),
    Report(Bytes, u32),
    ReportCount(Bytes),
    Appeal(Bytes),
    ModerationEntry(Bytes, u32),
    ModerationCount(Bytes),
    QueueEntry(u32),
    QueueSize,
    /// Maintained rolling aggregate for a subject.
    Aggregate(Address),
    /// Configured appeal window in ledgers.
    AppealWindowLedgers,
}

