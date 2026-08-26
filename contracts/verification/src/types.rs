use soroban_sdk::{contracttype, Address, Bytes, String, Vec};

/// Supported badge types.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BadgeType {
    /// Artist portfolio has been reviewed and verified.
    PortfolioVerified = 0,
    /// Artist identity has been verified.
    IdVerified = 1,
    /// Artist has completed a background check.
    BackgroundChecked = 2,
    /// Artist is a vetted professional agency.
    AgencyVerified = 3,
    /// Artist has achieved top-tier status.
    TopCreator = 4,
}

/// Lifecycle status of a badge.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BadgeStatus {
    /// Request submitted, awaiting admin review.
    Pending = 0,
    /// Approved and active.
    Active = 1,
    /// Rejected by admin.
    Rejected = 2,
    /// Revoked after prior approval.
    Revoked = 3,
    /// Passed the expiry ledger.
    Expired = 4,
}

/// A single badge record.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BadgeRecord {
    pub badge_id: Bytes,
    pub artist: Address,
    pub badge_type: BadgeType,
    pub status: BadgeStatus,
    /// Ledger at which the badge was requested.
    pub requested_ledger: u32,
    /// Ledger at which the badge expires (0 = no expiry).
    pub expiry_ledger: u32,
    /// Optional admin note (reason for rejection/revocation).
    pub note: Option<String>,
}

/// History entry kept every time a badge's status changes.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BadgeHistoryEntry {
    pub from_status: BadgeStatus,
    pub to_status: BadgeStatus,
    pub changed_at_ledger: u32,
    pub changed_by: Address,
    pub note: Option<String>,
}

/// On-chain key space.
#[contracttype]
pub enum DataKey {
    Admin,
    Badge(Bytes),
    /// History list for a badge_id.
    BadgeHistory(Bytes),
    /// Index: (artist, badge_type) → badge_id — to prevent duplicates.
    ArtistBadgeIndex(Address, BadgeType),
}
