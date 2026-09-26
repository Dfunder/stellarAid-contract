//! Rate Limiter Smart Contract
//!
//! Implements configurable rate limiting for account activities to prevent abuse.
//! This module can be used across other contracts to limit specific actions.

#![no_std]

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Duration, Map, Vec};

pub mod types;
pub mod errors;

use types::{RateLimitConfig, RateLimitKey, RateLimitRecord};
use errors::{RateLimitError, RateLimitErrorType};

include!("../../semver_types.rs");

const DEFAULT_COMMISSION_LIMIT: u32 = 5; // Max 5 commissions per artist per day
const DEFAULT_DISPUTE_LIMIT: u32 = 3;    // Max 3 disputes per user per day
const DEFAULT_ESCROW_LIMIT: u32 = 10;    // Max 10 escrows per user per day
const DEFAULT_WINDOW_LEDGERS: u32 = 28800; // ~24 hours at 5s/ledger

#[contract]
pub struct RateLimiter;

/// Initialize the rate limiter with default configurations
pub fn initialize_default_config(env: &Env, admin: Address) {
    admin.require_auth();

    let mut configs = Map::<RateLimitKey, RateLimitConfig>::new(env);

    // Set default limits
    configs.set(
        RateLimitKey::CommissionsPerArtist,
        RateLimitConfig {
            limit: DEFAULT_COMMISSION_LIMIT,
            window_ledgers: DEFAULT_WINDOW_LEDGERS,
        }
    );

    configs.set(
        RateLimitKey::DisputesPerUser,
        RateLimitConfig {
            limit: DEFAULT_DISPUTE_LIMIT,
            window_ledgers: DEFAULT_WINDOW_LEDGERS,
        }
    );

    configs.set(
        RateLimitKey::EscrowsPerUser,
        RateLimitConfig {
            limit: DEFAULT_ESCROW_LIMIT,
            window_ledgers: DEFAULT_WINDOW_LEDGERS,
        }
    );

    env.storage().instance().set(&types::DataKey::RateLimitConfig, &configs);

    env.events().publish(
        (symbol_short!("rl_init"),),
        (admin, DEFAULT_COMMISSION_LIMIT, DEFAULT_DISPUTE_LIMIT, DEFAULT_ESCROW_LIMIT),
    );
}

#[contractimpl]
impl RateLimiter {
    /// Initialize the rate limiter contract
    pub fn initialize(env: Env, admin: Address) -> Result<(), RateLimitError> {
        admin.require_auth();

        if env.storage().instance().has(&types::DataKey::Admin) {
            return Err(RateLimitError::AlreadyInitialized);
        }

        env.storage().instance().set(&types::DataKey::Admin, &admin);
        initialize_default_config(&env, admin);

        Ok(())
    }

    /// Set a new rate limit configuration
    pub fn set_limit(
        env: Env,
        admin: Address,
        key: RateLimitKey,
        limit: u32,
        window_ledgers: u32,
    ) -> Result<(), RateLimitError> {
        admin.require_auth();

        if limit == 0 {
            return Err(RateLimitError::InvalidLimit);
        }

        if window_ledgers == 0 {
            return Err(RateLimitError::InvalidWindow);
        }

        let mut configs = env.storage().instance()
            .get(&types::DataKey::RateLimitConfig)
            .unwrap_or_else(|| Map::<RateLimitKey, RateLimitConfig>::new(&env));

        configs.set(
            key.clone(),
            RateLimitConfig {
                limit,
                window_ledgers,
            }
        );

        env.storage().instance().set(&types::DataKey::RateLimitConfig, &configs);

        env.events().publish(
            (symbol_short!("rl_set"),),
            (key, limit, window_ledgers),
        );

        Ok(())
    }

    /// Get the current rate limit configuration for a key
    pub fn get_limit(env: Env, key: RateLimitKey) -> Option<RateLimitConfig> {
        let configs: Map<RateLimitKey, RateLimitConfig> = env.storage().instance()
            .get(&types::DataKey::RateLimitConfig)
            .unwrap_or_else(|| Map::<RateLimitKey, RateLimitConfig>::new(&env));

        configs.get(key)
    }

    /// Check if an action is allowed for an account
    pub fn check_rate_limit(
        env: Env,
        key: RateLimitKey,
        account: Address,
    ) -> Result<(), RateLimitError> {
        let config = match self.get_limit(env.clone(), key.clone()) {
            Some(c) => c,
            None => return Err(RateLimitError::NotInitialized),
        };

        let record_key = RateLimitRecord::key(key.clone(), account.clone());
        let current_ledger = env.ledger().sequence();

        // Get existing record or create a new one
        let mut record: RateLimitRecord = env.storage().persistent()
            .get(&record_key)
            .unwrap_or_else(|| RateLimitRecord {
                key: key.clone(),
                account: account.clone(),
                count: 0,
                first_ledger: current_ledger,
                last_ledger: current_ledger,
            });

        // Check if the window has expired
        if current_ledger - record.first_ledger > config.window_ledgers {
            // Reset the window
            record.count = 1;
            record.first_ledger = current_ledger;
            record.last_ledger = current_ledger;
        } else {
            // Increment the count
            record.count += 1;
            record.last_ledger = current_ledger;
        }

        // Check if the limit has been exceeded
        if record.count > config.limit {
            // Store the record with TTL
            env.storage().persistent().set(
                &record_key,
                &record,
            );

            // Set TTL to the end of the current window
            let ttl_ledgers = config.window_ledgers - (current_ledger - record.first_ledger);
            env.storage().persistent().extend_ttl(
                &record_key,
                ttl_ledgers,
                ttl_ledgers,
            );

            return Err(RateLimitError::LimitExceeded {
                key,
                limit: config.limit,
                window_ledgers: config.window_ledgers,
                current_count: record.count,
            });
        }

        // Store the record with TTL
        env.storage().persistent().set(
            &record_key,
            &record,
        );

        // Set TTL to the end of the current window
        let ttl_ledgers = config.window_ledgers - (current_ledger - record.first_ledger);
        env.storage().persistent().extend_ttl(
            &record_key,
            ttl_ledgers,
            ttl_ledgers,
        );

        // Emit event
        env.events().publish(
            (symbol_short!("rl_check"),),
            (key, account, record.count, config.limit),
        );

        Ok(())
    }

    /// Get the current count for a rate limit key and account
    pub fn get_count(env: Env, key: RateLimitKey, account: Address) -> Option<u32> {
        let record_key = RateLimitRecord::key(key.clone(), account.clone());
        let record: Option<RateLimitRecord> = env.storage().persistent().get(&record_key);

        match record {
            Some(r) => Some(r.count),
            None => None,
        }
    }

    /// Reset rate limits for a specific account (admin only)
    pub fn reset_limits(env: Env, admin: Address, account: Address) -> Result<(), RateLimitError> {
        admin.require_auth();

        // Get all possible keys
        let keys = vec![
            &env,
            RateLimitKey::CommissionsPerArtist,
            RateLimitKey::DisputesPerUser,
            RateLimitKey::EscrowsPerUser,
        ];

        // Remove all records for this account
        for key in keys.iter() {
            let record_key = RateLimitRecord::key(key.clone(), account.clone());
            env.storage().persistent().remove(&record_key);
        }

        env.events().publish(
            (symbol_short!("rl_reset"),),
            (account),
        );

        Ok(())
    }

    /// Get the current rate limit status for an account
    pub fn get_status(env: Env, key: RateLimitKey, account: Address) -> Option<RateLimitRecord> {
        let record_key = RateLimitRecord::key(key.clone(), account.clone());
        env.storage().persistent().get(&record_key)
    }

    /// Return the contract semantic version
    pub fn get_version(_env: Env) -> ContractVersion {
        parse_pkg_semver(env!("CARGO_PKG_VERSION"))
    }
}

#[cfg(test)]
mod test;
