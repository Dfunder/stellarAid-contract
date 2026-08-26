use soroban_sdk::{contracterror, symbol_short, Symbol};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum RecruitmentError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    JobNotFound = 4,
    JobExists = 5,
    JobNotOpen = 6,
    ApplicationNotFound = 7,
    AlreadyApplied = 8,
    InvalidStage = 9,
    InvalidRating = 10,
    InvalidBudget = 11,
    InvalidOpenings = 12,
    EmployerCannotApply = 13,
    TooManyApplicants = 14,
    NotHired = 15,
}

impl core::fmt::Display for RecruitmentError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::AlreadyInitialized => write!(f, "already initialized"),
            Self::NotInitialized => write!(f, "not initialized"),
            Self::Unauthorized => write!(f, "unauthorized"),
            Self::JobNotFound => write!(f, "job posting not found"),
            Self::JobExists => write!(f, "job posting already exists"),
            Self::JobNotOpen => write!(f, "job posting is not open"),
            Self::ApplicationNotFound => write!(f, "application not found"),
            Self::AlreadyApplied => write!(f, "applicant already applied"),
            Self::InvalidStage => write!(f, "invalid pipeline transition"),
            Self::InvalidRating => write!(f, "rating must be 1..=5"),
            Self::InvalidBudget => write!(f, "budget must be positive"),
            Self::InvalidOpenings => write!(f, "openings must be positive"),
            Self::EmployerCannotApply => write!(f, "employer cannot apply to own posting"),
            Self::TooManyApplicants => write!(f, "applicant limit reached"),
            Self::NotHired => write!(f, "applicant has not been hired"),
        }
    }
}

pub fn get_suggestion(error: RecruitmentError) -> Symbol {
    match error {
        RecruitmentError::AlreadyInitialized => symbol_short!("DUP"),
        RecruitmentError::NotInitialized => symbol_short!("NO_INIT"),
        RecruitmentError::Unauthorized => symbol_short!("AUTH"),
        RecruitmentError::JobNotFound => symbol_short!("NO_JOB"),
        RecruitmentError::JobExists => symbol_short!("JOB_DUP"),
        RecruitmentError::JobNotOpen => symbol_short!("CLOSED"),
        RecruitmentError::ApplicationNotFound => symbol_short!("NO_APP"),
        RecruitmentError::AlreadyApplied => symbol_short!("APP_DUP"),
        RecruitmentError::InvalidStage => symbol_short!("BAD_STAGE"),
        RecruitmentError::InvalidRating => symbol_short!("BAD_RATE"),
        RecruitmentError::InvalidBudget => symbol_short!("BAD_BUDG"),
        RecruitmentError::InvalidOpenings => symbol_short!("BAD_OPEN"),
        RecruitmentError::EmployerCannotApply => symbol_short!("SELF_APP"),
        RecruitmentError::TooManyApplicants => symbol_short!("TOO_MANY"),
        RecruitmentError::NotHired => symbol_short!("NOT_HIRED"),
    }
}
