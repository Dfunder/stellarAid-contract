use soroban_sdk::{contracttype, Address, Bytes, String};

/// On-chain key space for the reputation contract.
#[contracttype]
pub enum DataKey {
    /// Admin address (instance).
    Admin,
    /// `ReviewRecord` keyed by review_id.
    Review(Bytes),
    /// Running aggregate for an artist keyed by artist address.
    ArtistStats(Address),
    /// List of review_ids written by a specific client for an artist:
    /// used to prevent duplicates.
    ClientArtistReviews(Address, Address),
    // Review moderation & appeal — closes #604
    /// `ReportRecord` keyed by report_id.
    Report(Bytes),
    /// List of report_ids for a given review (review_id → Vec<Bytes>).
    ReviewReports(Bytes),
    /// `AppealRecord` keyed by appeal_id.
    Appeal(Bytes),
    /// `ModerationDecision` history for a review (review_id → Vec<ModerationDecision>).
    ModerationHistory(Bytes),
    /// Set of review_ids in the pending moderation queue (instance).
    ModerationQueue,
}

/// Moderation state of a review.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewStatus {
    /// Visible and counted in reputation.
    Active = 0,
    /// Hidden by moderator; not counted.
    Moderated = 1,
    /// Under dispute investigation.
    Disputed = 2,
}

/// A single review record stored on-chain.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewRecord {
    pub review_id: Bytes,
    pub artist: Address,
    pub client: Address,
    /// 1–100 inclusive.
    pub rating: u32,
    /// Short text comment (stored on-chain for transparency).
    pub comment: String,
    pub status: ReviewStatus,
    pub created_ledger: u32,
}

/// Running aggregate stats for an artist's reputation.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtistStats {
    /// Sum of all *active* ratings.
    pub total_score: u64,
    /// Number of active reviews.
    pub review_count: u32,
    /// Weighted reputation score (0–10_000 representing 0.00–100.00).
    /// Re-computed on every state change using a simple recency-weight formula.
    pub reputation_score: u32,
    /// Ledger of the most-recent active review (used for recency weighting).
    pub last_review_ledger: u32,
}

// ---------------------------------------------------------------------------
// Review reporting & appeal types — closes #604
// ---------------------------------------------------------------------------

/// Reason category for reporting a review.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportReason {
    Spam = 0,
    Abuse = 1,
    FalseInformation = 2,
    Harassment = 3,
    Other = 4,
}

/// A report submitted against a review.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReportRecord {
    pub report_id: Bytes,
    pub review_id: Bytes,
    pub reporter: Address,
    pub reason: ReportReason,
    pub details: String,
    pub created_ledger: u32,
}

/// Status of an appeal.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppealStatus {
    /// Appeal submitted, pending admin review.
    Pending = 0,
    /// Appeal accepted — review was reinstated or action reversed.
    Accepted = 1,
    /// Appeal rejected — original moderation decision upheld.
    Rejected = 2,
    /// Escalated to the dispute arbiter contract.
    Escalated = 3,
}

/// An appeal against a moderation decision.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppealRecord {
    pub appeal_id: Bytes,
    pub review_id: Bytes,
    pub appellant: Address,
    pub reason: String,
    pub status: AppealStatus,
    pub created_ledger: u32,
    /// Ledger at which the appeal was resolved (0 = unresolved).
    pub resolved_ledger: u32,
}

/// A single moderation decision recorded in the history log.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModerationDecision {
    pub action: ModerationAction,
    pub decided_ledger: u32,
    /// Optional appeal_id if this decision resulted from an appeal.
    pub appeal_id: Option<Bytes>,
}

/// The type of moderation action taken.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModerationAction {
    /// Review was hidden.
    Hidden = 0,
    /// Review was reinstated.
    Reinstated = 1,
    /// Dispute was opened.
    DisputeOpened = 2,
    /// Dispute was resolved in favour of the review (reinstated).
    DisputeReinstated = 3,
    /// Dispute was resolved against the review (kept moderated).
    DisputeRejected = 4,
    /// Appeal was accepted.
    AppealAccepted = 5,
    /// Appeal was rejected.
    AppealRejected = 6,
    /// Escalated to dispute arbiter.
    Escalated = 7,
}
