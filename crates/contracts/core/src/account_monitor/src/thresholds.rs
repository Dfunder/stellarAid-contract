#![no_std]
use soroban_sdk::{Env, u32};
use crate::storage::DataKey;

pub fn set_low_balance_threshold(env: &Env, threshold: u32) {
    env.storage().set(&DataKey::LowBalanceThreshold, &threshold);
}

pub fn get_low_balance_threshold(env: &Env) -> u32 {
    env.storage().get(&DataKey::LowBalanceThreshold).unwrap_or(0)
}