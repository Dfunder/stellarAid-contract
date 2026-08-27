use soroban_sdk::{contracterror, symbol_short, Symbol};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum AgreementError {
    AlreadyExists = 1,
    NotFound = 2,
    InvalidStatus = 3,
    Unauthorized = 4,
    InvalidAmount = 5,
    DeadlineInPast = 6,
    MilestoneBudgetExceeded = 7,
    NotAllMilestonesApproved = 8,
    ArithmeticOverflow = 9,
    /// Input string exceeds the allowed maximum length (closes #591).
    InputTooLong = 10,
    /// Deadline exceeds the maximum permitted future ledger (closes #592).
    DeadlineTooFar = 11,
    /// Milestone state transition is locked — a concurrent update is in
    /// progress; retry the operation. Closes #589.
    MilestoneLocked = 12,
    NotCancellable = 13,
    AlreadyCancelled = 14,
    InvalidPolicy = 15,
    AgencyExists = 16,
    AgencyNotFound = 17,
    ArtistNotOnRoster = 18,
    ArtistAlreadyRepresented = 19,
    InvalidSplitBps = 20,
    EmptyBatch = 21,
}

impl core::fmt::Display for AgreementError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::AlreadyExists => write!(f, "agreement already exists"),
            Self::NotFound => write!(f, "agreement not found"),
            Self::InvalidStatus => write!(f, "invalid status"),
            Self::Unauthorized => write!(f, "unauthorized"),
            Self::InvalidAmount => write!(f, "invalid amount"),
            Self::DeadlineInPast => write!(f, "deadline in past"),
            Self::MilestoneBudgetExceeded => write!(f, "milestone budget exceeded"),
            Self::NotAllMilestonesApproved => write!(f, "not all milestones approved"),
            Self::ArithmeticOverflow => write!(f, "arithmetic operation would overflow"),
            Self::InputTooLong => write!(f, "input string exceeds maximum allowed length"),
            Self::DeadlineTooFar => write!(f, "deadline exceeds the maximum permitted future date"),
            Self::MilestoneLocked => write!(f, "milestone is locked for concurrent update; retry"),
            Self::NotCancellable => write!(f, "agreement cannot be cancelled in this state"),
            Self::AlreadyCancelled => write!(f, "agreement is already cancelled"),
            Self::InvalidPolicy => write!(f, "invalid cancellation policy"),
            Self::AgencyExists => write!(f, "agency already registered"),
            Self::AgencyNotFound => write!(f, "agency not found"),
            Self::ArtistNotOnRoster => write!(f, "artist is not on this agency roster"),
            Self::ArtistAlreadyRepresented => write!(f, "artist is already represented"),
            Self::InvalidSplitBps => write!(f, "split bps out of range"),
            Self::EmptyBatch => write!(f, "batch must contain at least one payment"),
        }
    }
}

pub fn get_suggestion(error: AgreementError) -> Symbol {
    match error {
        AgreementError::AlreadyExists => symbol_short!("DUP"),
        AgreementError::NotFound => symbol_short!("NOT_FOUND"),
        AgreementError::InvalidStatus => symbol_short!("BAD_STS"),
        AgreementError::Unauthorized => symbol_short!("AUTH"),
        AgreementError::InvalidAmount => symbol_short!("BAD_AMT"),
        AgreementError::DeadlineInPast => symbol_short!("PAST_DDL"),
        AgreementError::MilestoneBudgetExceeded => symbol_short!("OVER_BUD"),
        AgreementError::NotAllMilestonesApproved => symbol_short!("NOT_ALL"),
        AgreementError::ArithmeticOverflow => symbol_short!("OVERFL"),
        AgreementError::InputTooLong => symbol_short!("TOO_LONG"),
        AgreementError::DeadlineTooFar => symbol_short!("FAR_DDL"),
        AgreementError::MilestoneLocked => symbol_short!("MS_LOCK"),
        AgreementError::NotCancellable => symbol_short!("NO_CANCEL"),
        AgreementError::AlreadyCancelled => symbol_short!("CANCELLED"),
        AgreementError::InvalidPolicy => symbol_short!("BAD_POL"),
        AgreementError::AgencyExists => symbol_short!("AGY_DUP"),
        AgreementError::AgencyNotFound => symbol_short!("NO_AGY"),
        AgreementError::ArtistNotOnRoster => symbol_short!("NO_ROSTER"),
        AgreementError::ArtistAlreadyRepresented => symbol_short!("REPPED"),
        AgreementError::InvalidSplitBps => symbol_short!("BAD_BPS"),
        AgreementError::EmptyBatch => symbol_short!("NO_BATCH"),
    }
}
