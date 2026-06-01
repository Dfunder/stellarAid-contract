#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CampaignError {
    Unauthorized = 1,

    InvalidDeadline = 2,

    ExtensionLimitExceeded = 3,

    CampaignNotExtendable = 4,
}