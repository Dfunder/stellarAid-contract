use soroban_sdk::{contracterror, symbol_short, Symbol};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum DisputeError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    NotFound = 4,
    InvalidStatus = 5,
    AlreadyResolved = 6,
    AutoResolveNotDue = 7,
    InvalidShareBps = 8,
    ArithmeticOverflow = 9,
    /// The resolution note exceeds `shared::validation::MAX_MEMO_LEN` (#765).
    /// The note is written to persistent storage, so it has to be bounded
    /// before the write, not after. Appended, never renumbered: error code
    /// values are permanent per `docs/UPGRADE_AND_ROLLBACK.md`.
    NoteTooLong = 10,
    /// Evidence arrived after the dispute's deadline (#977).
    EvidenceWindowClosed = 11,
    /// A submitted evidence blob exceeds `MAX_EVIDENCE_LEN` (#977).
    EvidenceTooLong = 12,
    /// The configured dispute TTL is zero or above the documented maximum
    /// (#977).
    InvalidTtl = 13,
}

impl core::fmt::Display for DisputeError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::AlreadyInitialized => write!(f, "already initialized"),
            Self::NotInitialized => write!(f, "not initialized"),
            Self::Unauthorized => write!(f, "unauthorized"),
            Self::NotFound => write!(f, "dispute not found"),
            Self::InvalidStatus => write!(f, "invalid status"),
            Self::AlreadyResolved => write!(f, "already resolved"),
            Self::AutoResolveNotDue => write!(f, "auto-resolve not yet due"),
            Self::InvalidShareBps => write!(f, "invalid share bps"),
            Self::ArithmeticOverflow => write!(f, "arithmetic operation would overflow"),
            Self::NoteTooLong => write!(f, "resolution note exceeds maximum length"),
            Self::EvidenceWindowClosed => write!(f, "the evidence window has closed"),
            Self::EvidenceTooLong => write!(f, "evidence exceeds maximum length"),
            Self::InvalidTtl => write!(f, "dispute TTL is zero or above the maximum"),
        }
    }
}

pub fn get_suggestion(error: DisputeError) -> Symbol {
    match error {
        DisputeError::AlreadyInitialized => symbol_short!("DUP"),
        DisputeError::NotInitialized => symbol_short!("NO_INIT"),
        DisputeError::Unauthorized => symbol_short!("AUTH"),
        DisputeError::NotFound => symbol_short!("NOT_FOUND"),
        DisputeError::InvalidStatus => symbol_short!("BAD_STS"),
        DisputeError::AlreadyResolved => symbol_short!("RESOLVED"),
        DisputeError::AutoResolveNotDue => symbol_short!("NOT_DUE"),
        DisputeError::InvalidShareBps => symbol_short!("BAD_BPS"),
        DisputeError::ArithmeticOverflow => symbol_short!("OVERFL"),
        DisputeError::NoteTooLong => symbol_short!("LONG_NOTE"),
    }
}
