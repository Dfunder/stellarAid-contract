use soroban_sdk::{contracttype, Address, Bytes, String, Vec};

pub const TOTAL_BPS: u32 = 10_000;
pub const MAX_PRIZE_POSITIONS: u32 = 10;

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompetitionStatus {
    /// Accepting submissions, then votes, according to the window in the rules.
    Open = 0,
    /// Voting closed and the ranking is fixed.
    Finalized = 1,
    /// Prizes paid out.
    Settled = 2,
}

/// Rules fixed at creation. The two windows run back to back: submissions
/// first, then voting, both measured from the creation ledger.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompetitionRules {
    pub submission_ledgers: u32,
    pub voting_ledgers: u32,
    pub max_submissions: u32,
    /// Reputation a voter needs before their ballot counts.
    pub min_reputation: u32,
    /// Prize shares by finishing position, in bps; must total 10000.
    pub prize_split_bps: Vec<u32>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Competition {
    pub id: Bytes,
    pub organizer: Address,
    pub token: Address,
    pub title: String,
    pub prize_pool: i128,
    pub rules: CompetitionRules,
    pub status: CompetitionStatus,
    pub submission_end_ledger: u32,
    pub voting_end_ledger: u32,
    pub submission_count: u32,
    pub total_votes: i128,
    pub created_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Submission {
    pub competition_id: Bytes,
    pub entrant: Address,
    pub entry_uri: String,
    /// Reputation-weighted votes received.
    pub votes: i128,
    pub voter_count: u32,
    pub submitted_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Winner {
    pub rank: u32,
    pub entrant: Address,
    pub votes: i128,
    pub prize: i128,
}

/// One line of the competition history, written when a competition finalizes.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompetitionSummary {
    pub competition_id: Bytes,
    pub organizer: Address,
    pub prize_pool: i128,
    pub submission_count: u32,
    pub total_votes: i128,
    pub top_entrant: Option<Address>,
    pub finalized_ledger: u32,
}

#[contracttype]
pub enum DataKey {
    Admin,
    HistoryLimit,
    Reputation(Address),
    Competition(Bytes),
    Entrants(Bytes),
    Submission(Bytes, Address),
    Voted(Bytes, Address),
    Winners(Bytes),
    History,
}
