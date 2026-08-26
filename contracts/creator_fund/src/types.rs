use soroban_sdk::{contracttype, Address, Bytes, String};

pub const TOTAL_BPS: u32 = 10_000;

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FundType {
    GrantPool = 0,
    EmergencyRelief = 1,
    PlatformInitiative = 2,
    MatchingPool = 3,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProposalStatus {
    Voting = 0,
    Approved = 1,
    Rejected = 2,
    Executed = 3,
}

/// Guardrails every allocation out of a fund must satisfy.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DistributionRule {
    /// Largest single payout, in bps of the fund balance at execution time.
    pub max_allocation_bps: u32,
    /// Balance that must remain in the fund after any payout.
    pub min_reserve: i128,
    /// Share of total contributed capital that must participate in a vote,
    /// in bps, for the result to count.
    pub quorum_bps: u32,
    /// How long a proposal stays open for voting.
    pub voting_ledgers: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fund {
    pub id: Bytes,
    pub fund_type: FundType,
    pub steward: Address,
    pub token: Address,
    pub balance: i128,
    pub total_contributed: i128,
    pub total_allocated: i128,
    pub contributor_count: u32,
    pub rule: DistributionRule,
    pub created_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Proposal {
    pub id: Bytes,
    pub fund_id: Bytes,
    pub proposer: Address,
    pub recipient: Address,
    pub amount: i128,
    pub status: ProposalStatus,
    pub votes_for: i128,
    pub votes_against: i128,
    pub created_ledger: u32,
    pub voting_ends_ledger: u32,
    pub memo: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Allocation {
    pub proposal_id: Bytes,
    pub recipient: Address,
    pub amount: i128,
    pub ledger: u32,
}

/// Balance snapshot taken on every contribution and payout, so fund growth can
/// be charted without replaying the event log.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GrowthPoint {
    pub ledger: u32,
    pub balance: i128,
    pub total_contributed: i128,
    pub total_allocated: i128,
}

#[contracttype]
pub enum DataKey {
    Admin,
    HistoryLimit,
    Fund(Bytes),
    Contribution(Bytes, Address),
    Growth(Bytes),
    Proposal(Bytes),
    Allocations(Bytes),
    Voted(Bytes, Address),
}
