use soroban_sdk::{contracterror, symbol_short, Symbol};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum RevenueError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    AgreementNotFound = 4,
    AgreementExists = 5,
    EmptySplit = 6,
    TooManyParticipants = 7,
    InvalidSplitTotal = 8,
    DuplicateParticipant = 9,
    InvalidAmount = 10,
    AgreementNotActive = 11,
    ArithmeticOverflow = 12,
}

impl core::fmt::Display for RevenueError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::AlreadyInitialized => write!(f, "already initialized"),
            Self::NotInitialized => write!(f, "not initialized"),
            Self::Unauthorized => write!(f, "unauthorized"),
            Self::AgreementNotFound => write!(f, "agreement not found"),
            Self::AgreementExists => write!(f, "agreement already exists"),
            Self::EmptySplit => write!(f, "split must have at least one participant"),
            Self::TooManyParticipants => write!(f, "too many participants"),
            Self::InvalidSplitTotal => write!(f, "shares must total 10000 bps"),
            Self::DuplicateParticipant => write!(f, "duplicate participant"),
            Self::InvalidAmount => write!(f, "amount must be positive"),
            Self::AgreementNotActive => write!(f, "agreement is not active"),
            Self::ArithmeticOverflow => write!(f, "arithmetic operation would overflow"),
        }
    }
}

pub fn get_suggestion(error: RevenueError) -> Symbol {
    match error {
        RevenueError::AlreadyInitialized => symbol_short!("DUP"),
        RevenueError::NotInitialized => symbol_short!("NO_INIT"),
        RevenueError::Unauthorized => symbol_short!("AUTH"),
        RevenueError::AgreementNotFound => symbol_short!("NOT_FOUND"),
        RevenueError::AgreementExists => symbol_short!("EXISTS"),
        RevenueError::EmptySplit => symbol_short!("NO_SPLIT"),
        RevenueError::TooManyParticipants => symbol_short!("TOO_MANY"),
        RevenueError::InvalidSplitTotal => symbol_short!("BAD_BPS"),
        RevenueError::DuplicateParticipant => symbol_short!("DUP_PART"),
        RevenueError::InvalidAmount => symbol_short!("BAD_AMT"),
        RevenueError::AgreementNotActive => symbol_short!("INACTIVE"),
        RevenueError::ArithmeticOverflow => symbol_short!("OVERFL"),
    }
}
