use soroban_sdk::{Address, Env};

use crate::types::{CampaignData, DataKey, DonorRecord, MilestoneData};

// ── Persistent storage helpers ────────────────────────────────────────────────

pub fn set_campaign(env: &Env, data: &CampaignData) {
    env.storage().persistent().set(&DataKey::CampaignData, data);
}

pub fn get_campaign(env: &Env) -> Option<CampaignData> {
    env.storage().persistent().get(&DataKey::CampaignData)
}

pub fn set_milestone(env: &Env, index: u32, data: &MilestoneData) {
    env.storage()
        .persistent()
        .set(&DataKey::MilestoneData(index), data);
}

pub fn get_milestone(env: &Env, index: u32) -> Option<MilestoneData> {
    env.storage()
        .persistent()
        .get(&DataKey::MilestoneData(index))
}

pub fn set_donor(env: &Env, donor: &Address, record: &DonorRecord) {
    env.storage()
        .persistent()
        .set(&DataKey::DonorData(donor.clone()), record);
}

pub fn get_donor(env: &Env, donor: &Address) -> Option<DonorRecord> {
    env.storage()
        .persistent()
        .get(&DataKey::DonorData(donor.clone()))
}

pub fn get_total_raised(env: &Env) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::TotalRaised)
        .unwrap_or(0)
}

pub fn set_total_raised(env: &Env, amount: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::TotalRaised, &amount);
}

// ── Temporary storage helpers ─────────────────────────────────────────────────

pub fn get_contract_status(env: &Env) -> Option<u32> {
    env.storage()
        .temporary()
        .get(&DataKey::ContractStatus)
}

pub fn set_contract_status(env: &Env, status: u32) {
    env.storage()
        .temporary()
        .set(&DataKey::ContractStatus, &status);
}
