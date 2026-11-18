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
    // ── Verification badges (#598) ──────────────────────────────────────
    Badge(Address, BadgeType),
    BadgeHistory(Address),
    BadgeTypes(Address),
}

/// The kind of verification a badge represents. New kinds can be appended
/// without disturbing existing badges, which are keyed by (artist, type).
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BadgeType {
    /// Backed by the portfolio quality-review workflow in this contract.
    PortfolioVerified = 0,
    /// Off-chain identity check (KYC-style), attested by a reviewer.
    IdVerified = 1,
    /// Awarded for sustained high ratings; source of truth lives off this
    /// contract (e.g. the reputation contract) — reviewers issue/revoke it.
    TopRated = 2,
    /// Professional credential or certification attested by a reviewer.
    ProfessionalCertified = 3,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BadgeStatus {
    Active = 0,
    Revoked = 1,
}

impl BadgeStatus {
    pub fn revoked(&self) -> bool {
        matches!(self, BadgeStatus::Revoked)
    }
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Badge {
    pub artist: Address,
    pub badge_type: BadgeType,
    pub issuer: Address,
    pub status: BadgeStatus,
    pub issued_ledger: u32,
    /// Ledger after which the badge is no longer valid. Zero means it never
    /// expires on its own (only explicit revocation ends it).
    pub expires_ledger: u32,
    pub revoke_reason: Option<String>,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BadgeAction {
    Issued = 0,
    Renewed = 1,
    Revoked = 2,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BadgeEvent {
    pub badge_type: BadgeType,
    pub action: BadgeAction,
    pub actor: Address,
    pub ledger: u32,
    pub note: Option<String>,
}
