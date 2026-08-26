use soroban_sdk::{contracterror, symbol_short, Symbol};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum FundError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    FundNotFound = 4,
    FundExists = 5,
    ProposalNotFound = 6,
    ProposalExists = 7,
    InvalidAmount = 8,
    InvalidRule = 9,
    NotContributor = 10,
    AlreadyVoted = 11,
    VotingClosed = 12,
    VotingOpen = 13,
    ProposalNotApproved = 14,
    QuorumNotMet = 15,
    ExceedsAllocationLimit = 16,
    ReserveBreached = 17,
    ArithmeticOverflow = 18,
}

impl core::fmt::Display for FundError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::AlreadyInitialized => write!(f, "already initialized"),
            Self::NotInitialized => write!(f, "not initialized"),
            Self::Unauthorized => write!(f, "unauthorized"),
            Self::FundNotFound => write!(f, "fund not found"),
            Self::FundExists => write!(f, "fund already exists"),
            Self::ProposalNotFound => write!(f, "proposal not found"),
            Self::ProposalExists => write!(f, "proposal already exists"),
            Self::InvalidAmount => write!(f, "amount must be positive"),
            Self::InvalidRule => write!(f, "invalid distribution rule"),
            Self::NotContributor => write!(f, "caller has no voting power in this fund"),
            Self::AlreadyVoted => write!(f, "already voted on this proposal"),
            Self::VotingClosed => write!(f, "voting period has closed"),
            Self::VotingOpen => write!(f, "voting period is still open"),
            Self::ProposalNotApproved => write!(f, "proposal is not approved"),
            Self::QuorumNotMet => write!(f, "quorum not met"),
            Self::ExceedsAllocationLimit => write!(f, "allocation exceeds per-payout cap"),
            Self::ReserveBreached => write!(f, "allocation would breach the fund reserve"),
            Self::ArithmeticOverflow => write!(f, "arithmetic operation would overflow"),
        }
    }
}

pub fn get_suggestion(error: FundError) -> Symbol {
    match error {
        FundError::AlreadyInitialized => symbol_short!("DUP"),
        FundError::NotInitialized => symbol_short!("NO_INIT"),
        FundError::Unauthorized => symbol_short!("AUTH"),
        FundError::FundNotFound => symbol_short!("NO_FUND"),
        FundError::FundExists => symbol_short!("FUND_DUP"),
        FundError::ProposalNotFound => symbol_short!("NO_PROP"),
        FundError::ProposalExists => symbol_short!("PROP_DUP"),
        FundError::InvalidAmount => symbol_short!("BAD_AMT"),
        FundError::InvalidRule => symbol_short!("BAD_RULE"),
        FundError::NotContributor => symbol_short!("NO_POWER"),
        FundError::AlreadyVoted => symbol_short!("VOTED"),
        FundError::VotingClosed => symbol_short!("V_CLOSED"),
        FundError::VotingOpen => symbol_short!("V_OPEN"),
        FundError::ProposalNotApproved => symbol_short!("NOT_APPR"),
        FundError::QuorumNotMet => symbol_short!("NO_QUORUM"),
        FundError::ExceedsAllocationLimit => symbol_short!("OVER_CAP"),
        FundError::ReserveBreached => symbol_short!("RESERVE"),
        FundError::ArithmeticOverflow => symbol_short!("OVERFL"),
    }
}
