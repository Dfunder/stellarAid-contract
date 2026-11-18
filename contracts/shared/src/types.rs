#![allow(unused)]
use soroban_sdk::{contracttype, Address, String, Vec};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum CommissionStatus {
    Locked = 0,
    Released = 1,
    Refunded = 2,
    Disputed = 3,
    Expired = 4,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum CampaignStatus {
    Pending = 0,
    Active = 1,
    Rejected = 2,
    Suspended = 3,
    Completed = 4,
    Cancelled = 5,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct Campaign {
    pub id: u64,
    pub owner: Address,
    pub goal: i128,
    pub raised: i128,
    pub status: CampaignStatus,
    pub deadline: u64,
    pub fee_bps: u32,
    pub platform_wallet: Option<Address>,
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
#[contracttype]
pub struct Withdrawal {
    pub campaign_id: u64,
    pub recipient: Address,
    pub amount: i128,
    pub approved: bool,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct DonationRefundedEvent {
    pub campaign_id: u64,
    pub donor: Address,
    pub amount: i128,
    pub caller: Address,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct AnonymousDonationEvent {
    pub campaign_id: u64,
    pub amount: i128,
}
