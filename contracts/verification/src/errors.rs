use soroban_sdk::{contracterror, symbol_short, Symbol};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum VerificationError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    PortfolioNotFound = 4,
    PortfolioExists = 5,
    InvalidStatus = 6,
    InvalidScore = 7,
    InvalidWorkCount = 8,
    InvalidInterval = 9,
    UpdateNotDue = 10,
    /// No badge of the requested type exists for this artist (#598).
    BadgeNotFound = 11,
    /// The badge has already been revoked (#598).
    BadgeAlreadyRevoked = 12,
}

impl core::fmt::Display for VerificationError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::AlreadyInitialized => write!(f, "already initialized"),
            Self::NotInitialized => write!(f, "not initialized"),
            Self::Unauthorized => write!(f, "unauthorized"),
            Self::PortfolioNotFound => write!(f, "portfolio not found"),
            Self::PortfolioExists => write!(f, "portfolio already submitted"),
            Self::InvalidStatus => write!(f, "invalid status"),
            Self::InvalidScore => write!(f, "score must be 0..=100"),
            Self::InvalidWorkCount => write!(f, "work count below minimum"),
            Self::InvalidInterval => write!(f, "invalid update interval"),
            Self::UpdateNotDue => write!(f, "portfolio update not yet due"),
            Self::BadgeNotFound => write!(f, "badge not found"),
            Self::BadgeAlreadyRevoked => write!(f, "badge is already revoked"),
        }
    }
}

pub fn get_suggestion(error: VerificationError) -> Symbol {
    match error {
        VerificationError::AlreadyInitialized => symbol_short!("DUP"),
        VerificationError::NotInitialized => symbol_short!("NO_INIT"),
        VerificationError::Unauthorized => symbol_short!("AUTH"),
        VerificationError::PortfolioNotFound => symbol_short!("NOT_FOUND"),
        VerificationError::PortfolioExists => symbol_short!("EXISTS"),
        VerificationError::InvalidStatus => symbol_short!("BAD_STS"),
        VerificationError::InvalidScore => symbol_short!("BAD_SCORE"),
        VerificationError::InvalidWorkCount => symbol_short!("BAD_WORK"),
        VerificationError::InvalidInterval => symbol_short!("BAD_INTVL"),
        VerificationError::UpdateNotDue => symbol_short!("NOT_DUE"),
        VerificationError::BadgeNotFound => symbol_short!("NO_BADGE"),
        VerificationError::BadgeAlreadyRevoked => symbol_short!("REVOKED"),
    }
}
