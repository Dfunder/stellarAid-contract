use soroban_sdk::{contracttype, Address, Bytes, String, Vec};

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgreementStatus {
    Pending = 0,
    Active = 1,
    Completed = 2,
    Cancelled = 3,
    Disputed = 4,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MilestoneStatus {
    Pending = 0,
    Approved = 1,
    Rejected = 2,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgreementRecord {
    pub commission_id: Bytes,
    pub client: Address,
    pub artist: Address,
    pub title: String,
    pub budget_usdc: i128,
    pub deadline_ledger: u32,
    pub status: AgreementStatus,
    pub created_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneRecord {
    pub milestone_id: Bytes,
    pub commission_id: Bytes,
    pub title: String,
    pub amount_usdc: i128,
    pub status: MilestoneStatus,
}

// ---------------------------------------------------------------------------
// Revision types — closes #600
// ---------------------------------------------------------------------------

/// Lifecycle of a revision request.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RevisionStatus {
    /// Submitted, awaiting the other party's response.
    Pending = 0,
    /// Accepted by the counterparty.
    Accepted = 1,
    /// Rejected by the counterparty.
    Rejected = 2,
    /// Timed out (deadline_ledger passed without response).
    Expired = 3,
}

/// Who proposed the revision — artist or client.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RevisionProposer {
    Artist = 0,
    Client = 1,
}

/// A single revision request record.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevisionRecord {
    pub revision_id: Bytes,
    pub commission_id: Bytes,
    pub proposer: RevisionProposer,
    /// Human-readable description of the requested change.
    pub description: String,
    /// Optional cost adjustment in USDC cents (positive = extra cost, negative = discount).
    pub cost_adjustment_usdc: i128,
    /// Ledger by which the counterparty must respond.
    pub deadline_ledger: u32,
    pub status: RevisionStatus,
    pub created_ledger: u32,
}

/// Per-agreement revision configuration and counter.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevisionConfig {
    /// Maximum number of revisions allowed (0 = unlimited).
    pub max_revisions: u32,
    /// Number of revisions used so far.
    pub used_revisions: u32,
}

// ---------------------------------------------------------------------------
// Team collaboration types — closes #603
// ---------------------------------------------------------------------------

/// Role a team member holds on a collaborative commission.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TeamRole {
    /// Full control — only one per agreement (the original artist).
    Lead = 0,
    /// Can contribute work; cannot modify agreement settings.
    Contributor = 1,
    /// Read-only access to progress updates.
    Viewer = 2,
}

/// A single team member record.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TeamMember {
    pub member: Address,
    pub role: TeamRole,
    /// Short label for attribution (e.g. "illustration", "animation").
    pub attribution: String,
    /// Ledger at which this member was added.
    pub added_ledger: u32,
}

/// Payment split entry: what share (in basis-points, 0–10_000) each member
/// receives from the artist's payout.  All entries must sum to exactly 10_000.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentSplitEntry {
    pub member: Address,
    /// Share in basis points (0–10_000).  Sum of all entries must equal 10_000.
    pub share_bps: u32,
}

#[contracttype]
pub enum DataKey {
    Agreement(Bytes),
    Milestone(Bytes, Bytes), // (commission_id, milestone_id)
    MilestonesForAgreement(Bytes),
    /// Serialization lock for milestone state transitions (closes #589).
    /// Key: (commission_id, milestone_id) — value: `true` when locked.
    MilestoneLock(Bytes, Bytes),
    // Cancellation (#605)
    CancellationPolicy(Bytes),
    Cancellation(Bytes),
    CancellationHistory,
    // Agency support (#609)
    Agency(Address),
    Roster(Address),
    RosterEntry(Address, Address),
    ArtistAgency(Address),
    AgencyAnalytics(Address),
    // Revision keys — closes #600
    Revision(Bytes, Bytes),              // (commission_id, revision_id)
    RevisionsForAgreement(Bytes),        // commission_id → Vec<RevisionRecord>
    RevisionConfig(Bytes),               // commission_id → RevisionConfig
    // Team collaboration keys — closes #603
    TeamMembers(Bytes),                  // commission_id → Vec<TeamMember>
    PaymentSplitConfig(Bytes),           // commission_id → Vec<PaymentSplitEntry>
}
