use soroban_sdk::{contracterror, symbol_short, Symbol};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum CompetitionError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    CompetitionNotFound = 4,
    CompetitionExists = 5,
    InvalidRules = 6,
    InvalidPrizePool = 7,
    SubmissionsClosed = 8,
    VotingNotOpen = 9,
    VotingClosed = 10,
    AlreadySubmitted = 11,
    SubmissionNotFound = 12,
    TooManySubmissions = 13,
    AlreadyVoted = 14,
    SelfVoteNotAllowed = 15,
    ReputationTooLow = 16,
    NotFinalized = 17,
    AlreadyFinalized = 18,
    AlreadySettled = 19,
    ArithmeticOverflow = 20,
}

impl core::fmt::Display for CompetitionError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::AlreadyInitialized => write!(f, "already initialized"),
            Self::NotInitialized => write!(f, "not initialized"),
            Self::Unauthorized => write!(f, "unauthorized"),
            Self::CompetitionNotFound => write!(f, "competition not found"),
            Self::CompetitionExists => write!(f, "competition already exists"),
            Self::InvalidRules => write!(f, "invalid competition rules"),
            Self::InvalidPrizePool => write!(f, "prize pool must be positive"),
            Self::SubmissionsClosed => write!(f, "submission window has closed"),
            Self::VotingNotOpen => write!(f, "voting has not opened yet"),
            Self::VotingClosed => write!(f, "voting window has closed"),
            Self::AlreadySubmitted => write!(f, "entrant has already submitted"),
            Self::SubmissionNotFound => write!(f, "submission not found"),
            Self::TooManySubmissions => write!(f, "submission limit reached"),
            Self::AlreadyVoted => write!(f, "voter has already voted"),
            Self::SelfVoteNotAllowed => write!(f, "entrants cannot vote for themselves"),
            Self::ReputationTooLow => write!(f, "reputation below the voting threshold"),
            Self::NotFinalized => write!(f, "competition has not been finalized"),
            Self::AlreadyFinalized => write!(f, "competition is already finalized"),
            Self::AlreadySettled => write!(f, "prizes have already been distributed"),
            Self::ArithmeticOverflow => write!(f, "arithmetic operation would overflow"),
        }
    }
}

pub fn get_suggestion(error: CompetitionError) -> Symbol {
    match error {
        CompetitionError::AlreadyInitialized => symbol_short!("DUP"),
        CompetitionError::NotInitialized => symbol_short!("NO_INIT"),
        CompetitionError::Unauthorized => symbol_short!("AUTH"),
        CompetitionError::CompetitionNotFound => symbol_short!("NO_COMP"),
        CompetitionError::CompetitionExists => symbol_short!("COMP_DUP"),
        CompetitionError::InvalidRules => symbol_short!("BAD_RULE"),
        CompetitionError::InvalidPrizePool => symbol_short!("BAD_POOL"),
        CompetitionError::SubmissionsClosed => symbol_short!("SUB_SHUT"),
        CompetitionError::VotingNotOpen => symbol_short!("V_EARLY"),
        CompetitionError::VotingClosed => symbol_short!("V_CLOSED"),
        CompetitionError::AlreadySubmitted => symbol_short!("SUB_DUP"),
        CompetitionError::SubmissionNotFound => symbol_short!("NO_SUB"),
        CompetitionError::TooManySubmissions => symbol_short!("TOO_MANY"),
        CompetitionError::AlreadyVoted => symbol_short!("VOTED"),
        CompetitionError::SelfVoteNotAllowed => symbol_short!("SELF_VOT"),
        CompetitionError::ReputationTooLow => symbol_short!("LOW_REP"),
        CompetitionError::NotFinalized => symbol_short!("NOT_FIN"),
        CompetitionError::AlreadyFinalized => symbol_short!("FINAL"),
        CompetitionError::AlreadySettled => symbol_short!("SETTLED"),
        CompetitionError::ArithmeticOverflow => symbol_short!("OVERFL"),
    }
}
