//! Audit log errors (closes #712).

use soroban_sdk::{contracterror, symbol_short, Symbol};

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditError {
    /// `initialize` has not run, so nothing may be appended.
    NotInitialized = 1,
    /// `initialize` has already run; the contract is single-shot.
    AlreadyInitialized = 2,
    /// Caller is not the platform admin, and cannot attest transactions.
    Unauthorized = 3,
    /// No entry exists for this sequence number.
    EntryNotFound = 4,
    /// No appends exist for this reference.
    ReferenceNotFound = 5,
    /// A negative amount cannot be recorded in an audit entry.
    InvalidAmount = 6,
    /// A `UserActivity` entry must not carry a token, and a transaction entry
    /// must.
    InvalidAction = 7,
    /// `page_size` was zero or above
    /// [`MAX_PAGE_SIZE`](crate::MAX_PAGE_SIZE).
    InvalidPageSize = 8,
    /// `from_ledger` is above `to_ledger`.
    InvalidLedgerRange = 9,
}

impl core::fmt::Display for AuditError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::NotInitialized => write!(f, "audit log is not initialized"),
            Self::AlreadyInitialized => write!(f, "audit log already initialized"),
            Self::Unauthorized => write!(f, "caller is not authorized to attest transactions"),
            Self::EntryNotFound => write!(f, "audit entry not found"),
            Self::ReferenceNotFound => write!(f, "no audit entries for this reference"),
            Self::InvalidAmount => write!(f, "amount must not be negative"),
            Self::InvalidAction => write!(f, "action and token do not match entry kind"),
            Self::InvalidPageSize => write!(f, "page size is out of range"),
            Self::InvalidLedgerRange => write!(f, "from_ledger must not exceed to_ledger"),
        }
    }
}

pub fn get_suggestion(error: AuditError) -> Symbol {
    match error {
        AuditError::NotInitialized => symbol_short!("NO_INIT"),
        AuditError::AlreadyInitialized => symbol_short!("DUP"),
        AuditError::Unauthorized => symbol_short!("AUTH"),
        AuditError::EntryNotFound => symbol_short!("NO_ENTRY"),
        AuditError::ReferenceNotFound => symbol_short!("NO_REF"),
        AuditError::InvalidAmount => symbol_short!("BAD_AMT"),
        AuditError::InvalidAction => symbol_short!("BAD_KIND"),
        AuditError::InvalidPageSize => symbol_short!("BAD_PAGE"),
        AuditError::InvalidLedgerRange => symbol_short!("BAD_RANGE"),
    }
}
