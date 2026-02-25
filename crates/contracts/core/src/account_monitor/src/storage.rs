#![no_std]
use soroban_sdk::{contracttype, Address};

#[contracttype]
pub enum DataKey {
    MasterAccount,
    TransactionCount,
    LowBalanceThreshold,
}