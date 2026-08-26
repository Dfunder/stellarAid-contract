use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum AnalyticsError {
    /// Contract not yet initialized.
    NotInitialized = 1,
    /// Contract already initialized.
    AlreadyInitialized = 2,
    /// Artist record not found.
    NotFound = 3,
    /// Caller is not authorized for this operation.
    Unauthorized = 4,
    /// Amount or rate value is out of valid range.
    InvalidValue = 5,
    /// Arithmetic overflow.
    ArithmeticOverflow = 6,
    /// Category string exceeds maximum length.
    CategoryTooLong = 7,
}

impl core::fmt::Display for AnalyticsError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::NotInitialized => write!(f, "contract not initialized"),
            Self::AlreadyInitialized => write!(f, "contract already initialized"),
            Self::NotFound => write!(f, "artist record not found"),
            Self::Unauthorized => write!(f, "caller not authorized"),
            Self::InvalidValue => write!(f, "invalid value"),
            Self::ArithmeticOverflow => write!(f, "arithmetic overflow"),
            Self::CategoryTooLong => write!(f, "category string too long"),
        }
    }
}
