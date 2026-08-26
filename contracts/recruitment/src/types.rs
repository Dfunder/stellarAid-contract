use soroban_sdk::{contracttype, Address, Bytes, String};

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobStatus {
    Open = 0,
    Closed = 1,
    Filled = 2,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stage {
    Applied = 0,
    Screening = 1,
    Interview = 2,
    Offered = 3,
    Hired = 4,
    Rejected = 5,
    Withdrawn = 6,
    Declined = 7,
}

impl Stage {
    /// Position in the forward funnel. Terminal stages have no position and are
    /// reached through their own dedicated entry points.
    pub fn rank(&self) -> Option<u32> {
        match self {
            Stage::Applied => Some(0),
            Stage::Screening => Some(1),
            Stage::Interview => Some(2),
            Stage::Offered => Some(3),
            Stage::Hired => Some(4),
            _ => None,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Stage::Hired | Stage::Rejected | Stage::Withdrawn | Stage::Declined
        )
    }
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Job {
    pub id: Bytes,
    pub employer: Address,
    pub title: String,
    pub budget: i128,
    pub openings: u32,
    pub filled: u32,
    pub applicant_count: u32,
    pub status: JobStatus,
    pub posted_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Offer {
    pub rate: i128,
    pub start_ledger: u32,
    pub made_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Application {
    pub job_id: Bytes,
    pub applicant: Address,
    pub proposal_uri: String,
    pub rate: i128,
    pub stage: Stage,
    pub applied_ledger: u32,
    pub updated_ledger: u32,
}

/// Live headcount per funnel stage, maintained as applications move so the
/// hiring pipeline can be read without scanning every application.
#[contracttype]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Pipeline {
    pub applied: u32,
    pub screening: u32,
    pub interview: u32,
    pub offered: u32,
    pub hired: u32,
    pub rejected: u32,
    pub withdrawn: u32,
    pub declined: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Performance {
    pub reviews: u32,
    pub total_rating: u32,
    pub last_rating: u32,
    pub last_ledger: u32,
    pub last_note: String,
}

#[contracttype]
pub enum DataKey {
    Admin,
    MaxApplicants,
    Job(Bytes),
    Applicants(Bytes),
    Pipeline(Bytes),
    Application(Bytes, Address),
    Offer(Bytes, Address),
    Performance(Bytes, Address),
}
