use soroban_sdk::{Env};

// Simple counters stored on-chain

pub fn increment_campaign_count(env: &Env) {
    let key = "campaign_count";
    let mut count: u64 = env.storage().instance().get(&key).unwrap_or(0);
    count += 1;
    env.storage().instance().set(&key, &count);
}

pub fn increment_total_donations(env: &Env, amount: i128) {
    let key = "total_donations";
    let mut total: i128 = env.storage().instance().get(&key).unwrap_or(0);
    total += amount;
    env.storage().instance().set(&key, &total);
}