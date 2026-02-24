#![no_std]

mod storage;
mod events;
mod thresholds;

use soroban_sdk::{contract, contractimpl, Env, Address, u32};

#[contract]
pub struct AccountMonitorContract;

#[contractimpl]
impl AccountMonitorContract {

    // Initialize with master account address and low balance threshold
    pub fn initialize(env: Env, master: Address, low_balance: u32) {
        if env.storage().has(&storage::DataKey::MasterAccount) {
            panic!("Already initialized");
        }
        env.storage().set(&storage::DataKey::MasterAccount, &master);
        env.storage().set(&storage::DataKey::TransactionCount, &0u32);
        thresholds::set_low_balance_threshold(&env, low_balance);
    }

    // Log a transaction
    pub fn log_transaction(env: Env) {
        let master: Address = env.storage().get(&storage::DataKey::MasterAccount).unwrap();
        let count: u32 = env.storage().get(&storage::DataKey::TransactionCount).unwrap_or(0);
        let new_count = count + 1;
        env.storage().set(&storage::DataKey::TransactionCount, &new_count);
        events::transaction_logged(&env, master, new_count);
    }

    // Check for low balance and emit alert if necessary
    pub fn check_low_balance(env: Env, current_balance: u32) {
        let master: Address = env.storage().get(&storage::DataKey::MasterAccount).unwrap();
        let threshold = thresholds::get_low_balance_threshold(&env);
        if current_balance < threshold {
            events::low_balance_alert(&env, master, current_balance);
        }
    }

    // Set / get threshold
    pub fn set_low_balance_threshold(env: Env, threshold: u32) {
        thresholds::set_low_balance_threshold(&env, threshold);
    }

    pub fn get_low_balance_threshold(env: Env) -> u32 {
        thresholds::get_low_balance_threshold(&env)
    }

    // Get transaction count
    pub fn get_transaction_count(env: Env) -> u32 {
        env.storage().get(&storage::DataKey::TransactionCount).unwrap_or(0)
    }
}