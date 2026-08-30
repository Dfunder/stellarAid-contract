use soroban_sdk::{contracterror, symbol_short, Symbol};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum AnalyticsError {
    /// Contract has already been initialized.
    AlreadyInitialized = 1,
    /// Contract has not been initialized yet.
    NotInitialized = 2,
    /// Caller is not the admin.
    Unauthorized = 3,
    /// An argument value is out of the accepted range.
    InvalidAmount = 4,
    /// No metrics found for the given artist.
    NotFound = 5,
    /// An arithmetic operation would overflow.
    ArithmeticOverflow = 6,
    /// The satisfaction score is outside the allowed 1–50 range (1–5 × 10).
    InvalidScore = 7,
}

impl core::fmt::Display for AnalyticsError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::AlreadyInitialized => write!(f, "contract already initialized"),
            Self::NotInitialized => write!(f, "contract not initialized"),
            Self::Unauthorized => write!(f, "caller is not the admin"),
            Self::InvalidAmount => write!(f, "amount must be greater than zero"),
            Self::NotFound => write!(f, "no metrics found for artist"),
            Self::ArithmeticOverflow => write!(f, "arithmetic operation would overflow"),
            Self::InvalidScore => write!(f, "satisfaction score must be between 10 and 50"),
        }
    }
}

#[allow(dead_code)]
pub fn get_suggestion(error: AnalyticsError) -> Symbol {
    match error {
        AnalyticsError::AlreadyInitialized => symbol_short!("DUP_INIT"),
        AnalyticsError::NotInitialized => symbol_short!("NO_INIT"),
        AnalyticsError::Unauthorized => symbol_short!("AUTH"),
        AnalyticsError::InvalidAmount => symbol_short!("BAD_AMT"),
        AnalyticsError::NotFound => symbol_short!("NOT_FOUND"),
        AnalyticsError::ArithmeticOverflow => symbol_short!("OVERFL"),
        AnalyticsError::InvalidScore => symbol_short!("BAD_SCR"),
    }
}
