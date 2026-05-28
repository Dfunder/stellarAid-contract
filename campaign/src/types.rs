use soroban_sdk::{contracttype, Address, BytesN, Vec};

// ── Supporting enums ─────────────────────────────────────────────────────────

/// Issue #167 – campaign lifecycle status
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CampaignStatus {
    Active,
    Successful,
    Failed,
    Cancelled,
}

/// Issue #168 – milestone release status
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MilestoneStatus {
    Pending,
    Released,
    Cancelled,
}

/// Accepted asset descriptor (native XLM or a Stellar asset)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssetInfo {
    Native,
    Stellar(Address),
}

// ── Issue #166 – storage key enum ────────────────────────────────────────────

/// All persistent storage keys used by the campaign contract.
/// Implements `contracttype` so Soroban can serialise it via XDR / `IntoVal`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    CampaignData,
    MilestoneData(u32),
    DonorData(Address),
    TotalRaised,
    ContractStatus,
}

// ── Issue #167 – CampaignData struct ─────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignData {
    pub creator: Address,
    pub goal_amount: i128,
    pub raised_amount: i128,
    pub end_time: u64,
    pub status: CampaignStatus,
    pub accepted_assets: Vec<AssetInfo>,
    pub milestone_count: u32,
}

// ── Issue #168 – MilestoneData struct ────────────────────────────────────────

/// Max 5 milestones enforced at the contract call site.
pub const MAX_MILESTONES: u32 = 5;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneData {
    pub index: u32,
    pub target_amount: i128,
    pub description_hash: BytesN<32>,
    pub status: MilestoneStatus,
    pub released_at: Option<u64>,
    pub release_tx: Option<BytesN<32>>,
}

// ── Issue #169 – DonorRecord struct ──────────────────────────────────────────

/// Stored under `DataKey::DonorData(donor_address)`.
/// Aggregated per-donor across multiple donations.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DonorRecord {
    pub donor: Address,
    pub total_donated: i128,
    pub asset: AssetInfo,
    pub last_donation_time: u64,
}
