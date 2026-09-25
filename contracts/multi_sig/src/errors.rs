//! Multi-signature authorization errors (closes #709).

use soroban_sdk::{contracterror, symbol_short, Symbol};

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MultiSigError {
    /// `initialize` has not run, so no configuration exists.
    NotInitialized = 1,
    /// `initialize` has already run; the contract is single-shot.
    AlreadyInitialized = 2,
    /// Caller is not the configured admin, or is not an authorised signer.
    Unauthorized = 3,
    /// The signer set is empty, oversized, or contains the same address twice.
    InvalidSigners = 4,
    /// The threshold is below [`MIN_THRESHOLD`](crate::types::MIN_THRESHOLD) or
    /// larger than the signer set, so it could never be met.
    InvalidThreshold = 5,
    /// The signature lifetime is zero or beyond
    /// [`MAX_EXPIRY_LEDGERS`](crate::types::MAX_EXPIRY_LEDGERS).
    InvalidExpiry = 6,
    /// No proposal exists for this id.
    ProposalNotFound = 7,
    /// A proposal already exists for this id.
    ProposalExists = 8,
    /// The caller is not in the configured signer set.
    NotASigner = 9,
    /// This signer has already approved this proposal.
    DuplicateSigner = 10,
    /// Not enough distinct signatures have been collected.
    ThresholdNotMet = 11,
    /// The signature window elapsed.
    ProposalExpired = 12,
    /// The proposal already reached a final status.
    ProposalClosed = 13,
    /// The action payload is empty, so there is nothing to authorise.
    InvalidAction = 14,
}

impl core::fmt::Display for MultiSigError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::NotInitialized => write!(f, "multi-sig is not initialized"),
            Self::AlreadyInitialized => write!(f, "multi-sig already initialized"),
            Self::Unauthorized => write!(f, "caller is not authorized"),
            Self::InvalidSigners => write!(f, "signer set is empty, oversized, or has duplicates"),
            Self::InvalidThreshold => write!(f, "threshold cannot be met by this signer set"),
            Self::InvalidExpiry => write!(f, "signature expiry is out of range"),
            Self::ProposalNotFound => write!(f, "proposal not found"),
            Self::ProposalExists => write!(f, "proposal already exists"),
            Self::NotASigner => write!(f, "caller is not an authorised signer"),
            Self::DuplicateSigner => write!(f, "signer has already approved this proposal"),
            Self::ThresholdNotMet => write!(f, "signature threshold not met"),
            Self::ProposalExpired => write!(f, "signature window has expired"),
            Self::ProposalClosed => write!(f, "proposal is already closed"),
            Self::InvalidAction => write!(f, "action payload must not be empty"),
        }
    }
}

pub fn get_suggestion(error: MultiSigError) -> Symbol {
    match error {
        MultiSigError::NotInitialized => symbol_short!("NO_INIT"),
        MultiSigError::AlreadyInitialized => symbol_short!("DUP"),
        MultiSigError::Unauthorized => symbol_short!("AUTH"),
        MultiSigError::InvalidSigners => symbol_short!("NO_SIGNS"),
        MultiSigError::InvalidThreshold => symbol_short!("THRESH"),
        MultiSigError::InvalidExpiry => symbol_short!("EXPIRY"),
        MultiSigError::ProposalNotFound => symbol_short!("NO_PROP"),
        MultiSigError::ProposalExists => symbol_short!("PROP_DUP"),
        MultiSigError::NotASigner => symbol_short!("NO_SIGNER"),
        MultiSigError::DuplicateSigner => symbol_short!("DUP_SIGN"),
        MultiSigError::ThresholdNotMet => symbol_short!("NO_THRESH"),
        MultiSigError::ProposalExpired => symbol_short!("EXPIRED"),
        MultiSigError::ProposalClosed => symbol_short!("CLOSED"),
        MultiSigError::InvalidAction => symbol_short!("ACTION"),
    }
}
