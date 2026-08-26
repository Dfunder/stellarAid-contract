use soroban_sdk::{contracterror, symbol_short, Symbol};

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EscrowError {
    AlreadyExists = 1,
    NotFound = 2,
    InvalidStatus = 3,
    Unauthorized = 4,
    InvalidAmount = 5,
    InvalidFeeBps = 6,
    DisputeAlreadyOpen = 7,
    NotExpired = 8,
    Reentrant = 9,
    InvalidAddress = 10,
    InsufficientBalance = 11,
    ArithmeticOverflow = 12,
    /// Contract is paused — operation not permitted (closes #594).
    ContractPaused = 13,
    InvalidSplit = 14,
    /// Milestone-specific errors — closes #601.
    MilestoneAlreadyExists = 15,
    MilestoneNotFound = 16,
    InvalidMilestoneStatus = 17,
    MilestoneBudgetExceeded = 18,
}

impl core::fmt::Display for EscrowError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::AlreadyExists => write!(f, "escrow already exists"),
            Self::NotFound => write!(f, "escrow not found"),
            Self::InvalidStatus => write!(f, "invalid escrow status for operation"),
            Self::Unauthorized => write!(f, "caller is not authorized"),
            Self::InvalidAmount => write!(f, "amount must be greater than zero"),
            Self::InvalidFeeBps => write!(f, "fee basis points out of allowed range"),
            Self::DisputeAlreadyOpen => write!(f, "a dispute is already open for this escrow"),
            Self::NotExpired => write!(f, "escrow has not expired yet"),
            Self::Reentrant => write!(f, "re-entrant call detected and rejected"),
            Self::InvalidAddress => write!(f, "client and artist addresses must be distinct"),
            Self::InsufficientBalance => write!(f, "client does not hold enough USDC"),
            Self::ArithmeticOverflow => write!(f, "arithmetic operation would overflow"),
            Self::ContractPaused => write!(f, "contract is paused; operation not permitted"),
            Self::InvalidSplit => write!(f, "cancellation split must equal the escrowed amount"),
            Self::MilestoneAlreadyExists => write!(f, "milestone already exists"),
            Self::MilestoneNotFound => write!(f, "milestone not found"),
            Self::InvalidMilestoneStatus => write!(f, "invalid milestone status for operation"),
            Self::MilestoneBudgetExceeded => write!(f, "milestone amounts exceed escrow total"),
        }
    }
}

pub fn get_suggestion(error: EscrowError) -> Symbol {
    match error {
        EscrowError::AlreadyExists => symbol_short!("DUP"),
        EscrowError::NotFound => symbol_short!("NOT_FOUND"),
        EscrowError::InvalidStatus => symbol_short!("BAD_STS"),
        EscrowError::Unauthorized => symbol_short!("AUTH"),
        EscrowError::InvalidAmount => symbol_short!("BAD_AMT"),
        EscrowError::InvalidFeeBps => symbol_short!("BAD_BPS"),
        EscrowError::DisputeAlreadyOpen => symbol_short!("DUP_DS"),
        EscrowError::NotExpired => symbol_short!("NOT_EXP"),
        EscrowError::Reentrant => symbol_short!("REENTRY"),
        EscrowError::InvalidAddress => symbol_short!("BAD_ADR"),
        EscrowError::InsufficientBalance => symbol_short!("NO_FUND"),
        EscrowError::ArithmeticOverflow => symbol_short!("OVERFL"),
        EscrowError::ContractPaused => symbol_short!("PAUSED"),
        EscrowError::InvalidSplit => symbol_short!("BAD_SPLT"),
        EscrowError::MilestoneAlreadyExists => symbol_short!("MS_DUP"),
        EscrowError::MilestoneNotFound => symbol_short!("MS_404"),
        EscrowError::InvalidMilestoneStatus => symbol_short!("MS_STS"),
        EscrowError::MilestoneBudgetExceeded => symbol_short!("MS_BUD"),
    }
}
