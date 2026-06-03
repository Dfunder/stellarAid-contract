use soroban_sdk::contracterror;

/// Campaign-specific error codes for deadline extension, cancellation, and
/// contract lifecycle operations that fall outside the core campaign flow.
///
/// This enum complements `crate::types::Error` and is used only by the
/// standalone helper functions in `contract.rs`.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CampaignError {
    /// Caller is not the campaign creator.
    Unauthorized = 1,
    /// The new deadline is not later than the current deadline.
    InvalidDeadline = 2,
    /// The deadline has already been extended the maximum number of times.
    ExtensionLimitExceeded = 3,
    /// The campaign is not in a state that allows deadline extension.
    CampaignNotExtendable = 4,
    /// Cannot cancel because the campaign still holds donor funds.
    CannotCancelWithFunds = 5,
    /// Campaign has already been cancelled — no further cancellation.
    CampaignAlreadyCancelled = 6,
    /// The campaign deadline has passed; operation is no longer permitted.
    CampaignEnded = 7,
}
// Errors are defined in `types::Error`.
// This file is intentionally left empty — the single canonical Error enum
// lives in types.rs to avoid duplicate definitions.

