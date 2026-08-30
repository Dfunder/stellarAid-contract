use soroban_sdk::{contracterror, symbol_short, Symbol};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ReputationError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    ReviewNotFound = 4,
    AlreadyReported = 5,
    InvalidRating = 6,
    InvalidStatus = 7,
    AppealAlreadyExists = 8,
    AppealNotFound = 9,
    ArithmeticOverflow = 10,
    DuplicateReview = 5,
    ReviewNotFound = 6,
    InvalidStatus = 7,
    CommentTooLong = 8,
}

impl core::fmt::Display for ReputationError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::AlreadyInitialized => write!(f, "contract already initialized"),
            Self::NotInitialized => write!(f, "contract not initialized"),
            Self::Unauthorized => write!(f, "caller is not authorized"),
            Self::ReviewNotFound => write!(f, "review not found"),
            Self::AlreadyReported => write!(f, "this reporter already reported this review"),
            Self::InvalidRating => write!(f, "rating must be between 10 and 50"),
            Self::InvalidStatus => write!(f, "operation not permitted in current review status"),
            Self::AppealAlreadyExists => write!(f, "an appeal already exists for this review"),
            Self::AppealNotFound => write!(f, "no appeal found for this review"),
            Self::ArithmeticOverflow => write!(f, "arithmetic operation would overflow"),
        }
    }
}

#[allow(dead_code)]
pub fn get_suggestion(error: ReputationError) -> Symbol {
    match error {
        ReputationError::AlreadyInitialized => symbol_short!("DUP_INIT"),
        ReputationError::NotInitialized => symbol_short!("NO_INIT"),
        ReputationError::Unauthorized => symbol_short!("AUTH"),
        ReputationError::ReviewNotFound => symbol_short!("NOT_FOUND"),
        ReputationError::AlreadyReported => symbol_short!("DUP_RPT"),
        ReputationError::InvalidRating => symbol_short!("BAD_RATE"),
        ReputationError::InvalidStatus => symbol_short!("BAD_STS"),
        ReputationError::AppealAlreadyExists => symbol_short!("DUP_APP"),
        ReputationError::AppealNotFound => symbol_short!("NO_APP"),
        ReputationError::ArithmeticOverflow => symbol_short!("OVERFL"),
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
