use soroban_sdk::{contracttype, Address, Bytes, String, Vec};

/// Criteria for filtering search results.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchFilter {
    /// Comma-separated skill tags (empty Bytes = no filter).
    pub skills: Bytes,
    /// Minimum price in USDC cents (0 = no minimum).
    pub min_price: i128,
    /// Maximum price in USDC cents (0 = no maximum).
    pub max_price: i128,
    /// Minimum reputation score 0–10000 (0 = no filter).
    pub min_reputation: u32,
}

/// Sort order for results.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SortOrder {
    /// Highest reputation first.
    ReputationDesc = 0,
    /// Lowest price first.
    PriceAsc = 1,
    /// Highest price first.
    PriceDesc = 2,
    /// Most recently indexed first.
    NewestFirst = 3,
}

/// An indexed artist profile.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtistProfile {
    pub artist: Address,
    /// Comma-separated skill tags stored as Bytes for on-chain efficiency.
    pub skills: Bytes,
    /// Starting price in USDC cents.
    pub min_price: i128,
    /// Reputation score (0–10000) copied from the reputation contract at
    /// index/update time.
    pub reputation_score: u32,
    /// Free-text metadata (title, description keywords) stored as Bytes.
    pub metadata: Bytes,
    /// Ledger when the profile was first indexed.
    pub indexed_ledger: u32,
    /// Ledger when the profile was last updated.
    pub updated_ledger: u32,
}

/// On-chain key space.
#[contracttype]
pub enum DataKey {
    Admin,
    /// ArtistProfile keyed by artist Address.
    Profile(Address),
    /// Running count of search queries (analytics).
    SearchCount,
    /// Running count of indexed profiles.
    ProfileCount,
}

/// A page of search results returned to callers.
#[contracttype]
#[derive(Clone, Debug)]
pub struct SearchPage {
    pub results: Vec<ArtistProfile>,
    /// Total profiles scanned (before filtering).
    pub total_scanned: u32,
    /// Number of results in this page.
    pub page_size: u32,
    pub page_number: u32,
}
