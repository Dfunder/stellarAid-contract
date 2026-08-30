use soroban_sdk::{contracttype, Address, Bytes, String};

/// A single earnings record tied to a completed commission.
///
/// Tracks the category/client context for per-category and per-client
/// earnings breakdowns (issue #602).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EarningsRecord {
    /// The artist whose earnings this record belongs to.
    pub artist: Address,
    /// Commission identifier this payout came from.
    pub commission_id: Bytes,
    /// Category label (e.g. "illustration", "ui-design").
    pub category: String,
    /// Client address associated with this commission.
    pub client: Address,
    /// Amount earned (in the platform's base token units).
    pub amount: i128,
    /// Ledger sequence when the payout was recorded.
    pub ledger: u32,
}

/// Aggregate performance metrics for a single artist.
///
/// Updated incrementally as commissions complete or fail.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtistMetrics {
    /// Total amount earned across all completed commissions.
    pub total_earnings: i128,
    /// Number of commissions completed successfully.
    pub completed_count: u32,
    /// Number of commissions cancelled or refunded.
    pub cancelled_count: u32,
    /// Sum of response-time ledgers (for average calculation).
    pub response_time_sum: u64,
    /// Number of data points in `response_time_sum`.
    pub response_time_count: u32,
    /// Sum of client satisfaction scores (1–5 scale, stored ×10 for precision).
    pub satisfaction_score_sum: u32,
    /// Number of satisfaction score entries.
    pub satisfaction_score_count: u32,
    /// Ledger when these metrics were last updated.
    pub last_updated_ledger: u32,
}

/// Storage data keys.
#[contracttype]
pub enum DataKey {
    /// Admin address.
    Admin,
    /// Per-artist aggregate metrics.  Key: artist address.
    Metrics(Address),
    /// Earnings log for an artist.  Key: (artist, index).
    Earning(Address, u32),
    /// Count of earnings entries for an artist.  Key: artist.
    EarningCount(Address),
}
