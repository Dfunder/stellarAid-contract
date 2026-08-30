use soroban_sdk::{contracterror, symbol_short, Symbol};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SearchError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    ListingNotFound = 4,
    InvalidPrice = 5,
    InvalidRating = 6,
    TooManySkills = 7,
    TooManyKeywords = 8,
    InvalidPageSize = 9,
    InvalidPriceRange = 10,
}

impl core::fmt::Display for SearchError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::AlreadyInitialized => write!(f, "already initialized"),
            Self::NotInitialized => write!(f, "not initialized"),
            Self::Unauthorized => write!(f, "unauthorized"),
            Self::ListingNotFound => write!(f, "listing not found"),
            Self::InvalidPrice => write!(f, "price must be non-negative"),
            Self::InvalidRating => write!(f, "rating must be 0..=100"),
            Self::TooManySkills => write!(f, "too many skill tags"),
            Self::TooManyKeywords => write!(f, "too many keyword tags"),
            Self::InvalidPageSize => write!(f, "page size out of range"),
            Self::InvalidPriceRange => write!(f, "min_price must not exceed max_price"),
        }
    }
}

pub fn get_suggestion(error: SearchError) -> Symbol {
    match error {
        SearchError::AlreadyInitialized => symbol_short!("DUP"),
        SearchError::NotInitialized => symbol_short!("NO_INIT"),
        SearchError::Unauthorized => symbol_short!("AUTH"),
        SearchError::ListingNotFound => symbol_short!("NOT_FOUND"),
        SearchError::InvalidPrice => symbol_short!("BAD_PRICE"),
        SearchError::InvalidRating => symbol_short!("BAD_RATE"),
        SearchError::TooManySkills => symbol_short!("MANY_SKL"),
        SearchError::TooManyKeywords => symbol_short!("MANY_KW"),
        SearchError::InvalidPageSize => symbol_short!("BAD_PAGE"),
        SearchError::InvalidPriceRange => symbol_short!("BAD_RANGE"),
    }
}
