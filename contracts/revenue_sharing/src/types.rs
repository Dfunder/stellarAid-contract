use soroban_sdk::{contracttype, Address, Bytes, String};

pub const TOTAL_BPS: u32 = 10_000;
pub const MAX_PARTICIPANTS: u32 = 20;

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgreementStatus {
    Active = 0,
    Paused = 1,
    Terminated = 2,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Participant {
    pub account: Address,
    pub share_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Agreement {
    pub id: Bytes,
    pub owner: Address,
    pub token: Address,
    pub status: AgreementStatus,
    /// Bumped every time the split terms are replaced, so each revenue entry
    /// can be traced back to the terms that were in force when it was booked.
    pub terms_version: u32,
    pub total_revenue: i128,
    pub total_distributed: i128,
    pub entry_count: u32,
    pub created_ledger: u32,
    pub updated_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevenueEntry {
    pub sequence: u32,
    pub source: Address,
    pub gross: i128,
    pub distributed: i128,
    pub terms_version: u32,
    pub ledger: u32,
    pub memo: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevenueReport {
    pub total_revenue: i128,
    pub total_distributed: i128,
    pub entry_count: u32,
    pub terms_version: u32,
    pub status: AgreementStatus,
}

#[contracttype]
pub enum DataKey {
    Admin,
    HistoryLimit,
    Agreement(Bytes),
    Splits(Bytes),
    Earnings(Bytes, Address),
    History(Bytes),
}
