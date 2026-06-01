#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CampaignError {
    Unauthorized = 1,

    InvalidDeadline = 2,

    ExtensionLimitExceeded = 3,

    CampaignNotExtendable = 4,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CampaignError {
    Unauthorized = 1,

    CannotCancelWithFunds = 2,

    CampaignAlreadyCancelled = 3,
}

