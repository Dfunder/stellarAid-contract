use soroban_sdk::{contracttype, Address, String, Vec};

/// A single artist's searchable listing: what they can be found by, and the
/// facts used to filter/sort/rank them.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtistListing {
    pub artist: Address,
    /// Tags such as "illustration", "3d", "logo-design" — matched exactly by
    /// `SearchFilters::skill`.
    pub skills: Vec<String>,
    /// Starting/base rate, in the platform's smallest token unit. Used for
    /// price-range filtering and price sorting.
    pub price: i128,
    /// 0..=100 rating snapshot. Set by the admin (typically synced from the
    /// reputation contract off-chain, or by an admin bridge call), never by
    /// the artist themselves, so listings can't self-inflate their ranking.
    pub rating: u32,
    /// Free-form keyword tags describing the artist's portfolio/services —
    /// the "full-text search metadata" this listing is discoverable by.
    /// This is a keyword/tag index rather than arbitrary substring search:
    /// realistic on-chain full-text search would require indexing
    /// infrastructure this contract does not attempt to replace.
    pub keywords: Vec<String>,
    pub indexed_ledger: u32,
    /// Inactive listings are excluded from search results but keep their
    /// history (rating, prior keywords) until reactivated or re-indexed.
    pub active: bool,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SortBy {
    PriceAsc = 0,
    PriceDesc = 1,
    RatingDesc = 2,
    RatingAsc = 3,
    Newest = 4,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchFilters {
    pub skill: Option<String>,
    pub min_price: Option<i128>,
    pub max_price: Option<i128>,
    pub min_rating: Option<u32>,
    /// Exact-match against one of the listing's `keywords`.
    pub keyword: Option<String>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchResultPage {
    pub results: Vec<ArtistListing>,
    /// Total listings matching the filters, before pagination — lets a
    /// client compute how many pages exist.
    pub total_matches: u32,
    pub page: u32,
    pub page_size: u32,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub struct SearchAnalytics {
    pub total_searches: u64,
    pub total_indexed: u32,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Listing(Address),
    /// All artist addresses ever indexed, in indexing order. Search scans
    /// this list and filters/sorts/paginates in memory.
    AllArtists,
    Analytics,
}
