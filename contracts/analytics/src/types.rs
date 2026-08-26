use soroban_sdk::{contracttype, Address, Bytes, String};

// ---------------------------------------------------------------------------
// Storage key space
// ---------------------------------------------------------------------------

#[contracttype]
pub enum DataKey {
    /// Admin address (instance).
    Admin,
    /// Artist-level aggregate statistics.
    ArtistStats(Address),
    /// Earnings breakdown by category for an artist:
    /// keyed by (artist, category_tag).
    CategoryEarnings(Address, String),
    /// List of category tags recorded for an artist.
    ArtistCategories(Address),
    /// Project-completion snapshot keyed by artist.
    CompletionStats(Address),
    /// Response-time aggregate keyed by artist.
    ResponseTimeStats(Address),
    /// Client satisfaction trend data keyed by artist.
    SatisfactionTrend(Address),
    /// Earnings prediction snapshot keyed by artist.
    EarningsPrediction(Address),
}

// ---------------------------------------------------------------------------
// Aggregate artist statistics
// ---------------------------------------------------------------------------

/// Top-level performance metrics for an artist.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtistStats {
    /// Total USDC (smallest denomination) earned across all projects.
    pub total_earnings: i128,
    /// Total number of projects started (including in-flight).
    pub projects_started: u32,
    /// Total number of projects completed.
    pub projects_completed: u32,
    /// Total number of projects cancelled or abandoned.
    pub projects_cancelled: u32,
    /// Sum of all response times (in ledgers) for calculating average.
    pub total_response_time_ledgers: u64,
    /// Number of response-time samples collected.
    pub response_time_samples: u32,
    /// Sum of client satisfaction ratings (1–100 each).
    pub total_satisfaction: u64,
    /// Number of satisfaction ratings collected.
    pub satisfaction_count: u32,
    /// Ledger of last update.
    pub last_updated_ledger: u32,
}

/// Per-category earnings record.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CategoryEarnings {
    pub category: String,
    /// Total USDC earned in this category.
    pub earnings: i128,
    /// Number of completed projects in this category.
    pub project_count: u32,
}

/// Project completion rate snapshot.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionStats {
    pub started: u32,
    pub completed: u32,
    pub cancelled: u32,
    /// Rate in basis points (0–10_000 = 0%–100%).
    pub completion_rate_bps: u32,
}

/// Response time aggregate.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponseTimeStats {
    /// Average response time in ledgers (0 if no samples).
    pub avg_response_time_ledgers: u32,
    pub sample_count: u32,
}

/// A single client satisfaction data point stored in the trend log.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SatisfactionDataPoint {
    /// Commission identifier (opaque bytes).
    pub commission_id: Bytes,
    /// Rating 1–100.
    pub rating: u32,
    /// Ledger at which it was recorded.
    pub recorded_ledger: u32,
}

/// Earnings prediction snapshot.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EarningsPrediction {
    /// Predicted monthly earnings in USDC smallest-denomination units.
    /// Computed as (total_earnings / months_active) rounded down.
    pub predicted_monthly_earnings: i128,
    /// Number of ledger-months used to derive the prediction.
    pub months_active: u32,
    /// Ledger at which this prediction was computed.
    pub computed_ledger: u32,
}
