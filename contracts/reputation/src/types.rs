use soroban_sdk::{contracttype, Address, String};

/// Lifecycle of a single review.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewStatus {
    /// Freshly submitted, counts toward the artist's reputation.
    Active = 0,
    /// The artist has disputed this review; excluded from scoring while open.
    Disputed = 1,
    /// A moderator resolved the dispute in the reviewer's favor — counts again.
    Upheld = 2,
    /// A moderator removed the review (either resolving a dispute against it,
    /// or direct moderation for abuse/spam); permanently excluded from scoring.
    Removed = 3,
}

impl ReviewStatus {
    /// Whether a review in this status contributes to the reputation score.
    pub fn counts_toward_score(&self) -> bool {
        matches!(self, ReviewStatus::Active | ReviewStatus::Upheld)
    }
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Review {
    pub client: Address,
    pub artist: Address,
    /// Rating on a 1..=5 scale.
    pub rating: u32,
    pub comment: String,
    pub status: ReviewStatus,
    pub ledger: u32,
    /// Reason given by the artist when disputing this review.
    pub dispute_reason: Option<String>,
    /// Reason a moderator gave when resolving a dispute or moderating directly.
    pub moderation_note: Option<String>,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Moderator(Address),
    /// All reviews received by an artist, in submission order.
    ReviewsForArtist(Address),
    /// Dedup guard: has this client already reviewed this artist?
    HasReviewed(Address, Address), // (artist, client)
    /// Cached 0..=100 reputation score, recomputed on every review change.
    ReputationScore(Address),
}
