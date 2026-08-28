use soroban_sdk::{contracterror, symbol_short, Symbol};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ReputationError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidRating = 4,
    DuplicateReview = 5,
    ReviewNotFound = 6,
    InvalidStatus = 7,
    CommentTooLong = 8,
}

impl core::fmt::Display for ReputationError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::AlreadyInitialized => write!(f, "already initialized"),
            Self::NotInitialized => write!(f, "not initialized"),
            Self::Unauthorized => write!(f, "unauthorized"),
            Self::InvalidRating => write!(f, "rating must be 1..=5"),
            Self::DuplicateReview => write!(f, "client has already reviewed this artist"),
            Self::ReviewNotFound => write!(f, "review not found"),
            Self::InvalidStatus => write!(f, "invalid review status for this operation"),
            Self::CommentTooLong => write!(f, "comment exceeds maximum length"),
        }
    }
}

pub fn get_suggestion(error: ReputationError) -> Symbol {
    match error {
        ReputationError::AlreadyInitialized => symbol_short!("DUP"),
        ReputationError::NotInitialized => symbol_short!("NO_INIT"),
        ReputationError::Unauthorized => symbol_short!("AUTH"),
        ReputationError::InvalidRating => symbol_short!("BAD_RATE"),
        ReputationError::DuplicateReview => symbol_short!("DUP_REV"),
        ReputationError::ReviewNotFound => symbol_short!("NOT_FOUND"),
        ReputationError::InvalidStatus => symbol_short!("BAD_STS"),
        ReputationError::CommentTooLong => symbol_short!("TOO_LONG"),
    }
}
