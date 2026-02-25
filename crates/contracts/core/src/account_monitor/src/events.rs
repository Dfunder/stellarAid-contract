#![no_std]
use soroban_sdk::{Address, Env, symbol_short};

pub fn low_balance_alert(env: &Env, account: Address, balance: u32) {
    env.events().publish((symbol_short!("low_balance"),), (account, balance));
}

pub fn transaction_logged(env: &Env, account: Address, tx_count: u32) {
    env.events().publish((symbol_short!("tx_logged"),), (account, tx_count));
}