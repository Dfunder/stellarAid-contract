#![allow(clippy::too_many_arguments)]

use soroban_sdk::{contracttype, Address, String};

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Campaign {
    pub id: u64,
    pub owner: Address,
    pub goal: i128,
    pub raised: i128,
    pub status: CampaignStatus,
    pub deadline: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Donation {
    pub donor: Address,
    pub campaign_id: u64,
    pub amount: i128,
    pub timestamp: u64,
    pub memo: Option<String>,
    pub anonymous: bool,
    pub token_address: Option<Address>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Withdrawal {
    pub campaign_id: u64,
    pub recipient: Address,
    pub amount: i128,
    pub approved: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum CampaignStatus {
    Active = 0,
    Completed = 1,
    Suspended = 2,
    Rejected = 3,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct DonationRefundedEvent {
    pub campaign_id: u64,
    pub donor: Address,
    pub amount: i128,
    pub caller: Address,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct AnonymousDonationEvent {
    pub campaign_id: u64,
    pub amount: i128,
}
