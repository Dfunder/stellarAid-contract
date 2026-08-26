use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SearchError {
    /// Contract not yet initialized.
    NotInitialized = 1,
    /// Contract already initialized.
    AlreadyInitialized = 2,
    /// Profile or commission not found.
    NotFound = 3,
    /// Caller is not authorized.
    Unauthorized = 4,
    /// Page size must be > 0 and ≤ 50.
    InvalidPageSize = 5,
    /// Price range is invalid (min > max).
    InvalidPriceRange = 6,
    /// Profile already indexed.
    AlreadyIndexed = 7,
}
