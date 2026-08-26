use soroban_sdk::{contracttype, Address, String};

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortfolioStatus {
    Submitted = 0,
    UnderReview = 1,
    Verified = 2,
    Rejected = 3,
    UpdateRequired = 4,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewOutcome {
    Approved = 0,
    Rejected = 1,
    Resubmitted = 2,
}

/// Per-criterion marks, each on a 0..=100 scale. The weighted blend of these
/// is the overall quality score compared against the configured minimum.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QualityScore {
    pub originality: u32,
    pub technique: u32,
    pub consistency: u32,
    pub presentation: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Portfolio {
    pub artist: Address,
    pub metadata_uri: String,
    pub work_count: u32,
    pub status: PortfolioStatus,
    pub score: u32,
    pub revision: u32,
    pub submitted_ledger: u32,
    pub reviewed_ledger: u32,
    pub reviewer: Option<Address>,
    /// Ledger after which a verified portfolio must be refreshed. Zero while
    /// the portfolio has never been approved.
    pub next_update_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerificationRecord {
    pub revision: u32,
    pub outcome: ReviewOutcome,
    pub score: u32,
    pub quality: QualityScore,
    pub reviewer: Option<Address>,
    pub ledger: u32,
    pub note: String,
}

#[contracttype]
pub enum DataKey {
    Admin,
    MinScore,
    MinWorkCount,
    UpdateInterval,
    HistoryLimit,
    Reviewer(Address),
    Portfolio(Address),
    History(Address),
}
