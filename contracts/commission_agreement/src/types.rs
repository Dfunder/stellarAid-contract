use soroban_sdk::{contracttype, Address, Bytes, String};

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
    // Revisions (#600)
    RevisionPolicy(Bytes),
    RevisionsForAgreement(Bytes),
}
