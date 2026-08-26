use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum VerificationError {
    /// Contract not yet initialized.
    NotInitialized = 1,
    /// Contract already initialized.
    AlreadyInitialized = 2,
    /// Badge not found.
    NotFound = 3,
    /// Badge request already exists for this artist + badge type.
    AlreadyRequested = 4,
    /// Badge is not in the expected state for this operation.
    InvalidStatus = 5,
    /// Caller is not authorized.
    Unauthorized = 6,
    /// Badge has expired.
    BadgeExpired = 7,
    /// Expiry ledger is in the past.
    ExpiryInPast = 8,
}
