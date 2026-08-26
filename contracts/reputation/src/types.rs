use soroban_sdk::{contracttype, Address, Bytes, String};

/// The current state of a review in the moderation system.
///
/// Closes #604 – review moderation state machine.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewStatus {
    /// Review is publicly visible (no reports, or reports cleared).
    Active = 0,
    /// Review has been reported and is queued for admin review.
    UnderReview = 1,
    /// Admin has removed this review (spam, abuse, etc.).
    Removed = 2,
    /// Admin has cleared all reports; review is publicly visible again.
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
///
/// Closes #604 – appeal mechanism.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppealStatus {
    /// Appeal filed but not yet reviewed.
    Pending = 0,
    /// Appeal upheld; review re-instated (Active).
    Upheld = 1,
    /// Appeal denied; original moderation decision stands.
    Denied = 2,
    /// Appeal escalated to the on-chain dispute arbiter.
    Escalated = 3,
}

/// A review submitted by a client about an artist.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewRecord {
    pub review_id: Bytes,
    /// The artist being reviewed.
    pub artist: Address,
    /// The client who authored the review.
    pub reviewer: Address,
    /// Star rating, stored × 10 (e.g. 45 = 4.5 stars). Range: 10–50.
    pub rating_x10: u32,
    /// Free-text review body.
    pub comment: String,
    /// Current moderation state.
    pub status: ReviewStatus,
    /// Ledger when the review was submitted.
    pub created_ledger: u32,
}

/// A moderation report filed against a review.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReportRecord {
    pub review_id: Bytes,
    /// Address of the reporter.
    pub reporter: Address,
    pub reason: ReportReason,
    /// Optional additional context.
    pub details: String,
    pub created_ledger: u32,
}

/// An appeal record filed by the artist or reviewer.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppealRecord {
    pub review_id: Bytes,
    /// Who is appealing (artist or the original reviewer).
    pub appellant: Address,
    pub reason: String,
    pub status: AppealStatus,
    pub created_ledger: u32,
}

/// A moderation action taken by the admin on a review.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModerationRecord {
    pub review_id: Bytes,
    pub admin: Address,
    /// The status assigned by this decision.
    pub new_status: ReviewStatus,
    pub notes: String,
    pub ledger: u32,
}

/// Storage data keys.
#[contracttype]
pub enum DataKey {
    Admin,
    Review(Bytes),
    /// Reports for a review.  Key: (review_id, report_index).
    Report(Bytes, u32),
    /// Count of reports for a review.
    ReportCount(Bytes),
    /// Appeal for a review (at most one active appeal per review).
    Appeal(Bytes),
    /// Moderation history entry.  Key: (review_id, entry_index).
    ModerationEntry(Bytes, u32),
    /// Count of moderation entries for a review.
    ModerationCount(Bytes),
    /// Queue of review_ids currently under review (index → review_id).
    QueueEntry(u32),
    QueueSize,
}
