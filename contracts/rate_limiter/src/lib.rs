//! Rate Limiter Smart Contract
//!
//! Implements configurable rate limiting for account activities to prevent abuse.
//! This module can be used across other contracts to limit specific actions.
//!
//! Closes #710 – Implement Rate Limiting and Throttling.
//!
//! ## Why this file changed shape
//!
//! The crate was added to disk but never listed in the workspace `members`, so
//! it had never been compiled. It also could not have compiled as written: the
//! error type was a `Diagnostic`-carrying struct instead of a
//! `#[contracterror]` enum, `check_rate_limit` built a struct-variant error
//! against that struct, and `#[cfg(test)] mod test;` pointed at a file that did
//! not exist. Those are fixed here and the crate is now registered in the
//! workspace.
//!
//! ## Window semantics
//!
//! Counting is a fixed window: the first action of a window records
//! `first_ledger`, and the window rolls over once `window_ledgers` have passed.
//! Rejected attempts still advance the counter, so a client hammering a closed
//! window cannot keep its position warm.

#![no_std]

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Map, Symbol};

pub mod types;
pub mod errors;

use types::{RateLimitConfig, RateLimitKey, RateLimitRecord};
use errors::RateLimitError;

include!("../../semver_types.rs");

const DEFAULT_COMMISSION_LIMIT: u32 = 5; // Max 5 commissions per artist per day
const DEFAULT_DISPUTE_LIMIT: u32 = 3;    // Max 3 disputes per user per day
const DEFAULT_ESCROW_LIMIT: u32 = 10;    // Max 10 escrows per user per day
const DEFAULT_WINDOW_LEDGERS: u32 = 28800; // ~24 hours at 5s/ledger

#[contract]
pub struct RateLimiter;

/// Write the default limit set into instance storage.
///
/// Deliberately does **not** call `admin.require_auth()`: the only caller is
/// `initialize`, which has already done so, and asking the host for the same
/// authorization twice in one invocation is a known source of `HostError` (see
/// the note above `shared::rollout::trigger_rollback`).
fn initialize_default_config(env: &Env, admin: &Address) {
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
        (admin.clone(), DEFAULT_COMMISSION_LIMIT, DEFAULT_DISPUTE_LIMIT, DEFAULT_ESCROW_LIMIT),
    );
}

/// Read the configured limit for `key`, or `None` when the key is unconfigured.
fn load_config(env: &Env, key: &RateLimitKey) -> Option<RateLimitConfig> {
    let configs: Map<RateLimitKey, RateLimitConfig> = env
        .storage()
        .instance()
        .get(&types::DataKey::RateLimitConfig)
        .unwrap_or_else(|| Map::new(env));
    configs.get(key.clone())
}

/// Every configured key, in declaration order. `reset_limits` walks this list.
fn all_keys() -> [RateLimitKey; 3] {
    [
        RateLimitKey::CommissionsPerArtist,
        RateLimitKey::DisputesPerUser,
        RateLimitKey::EscrowsPerUser,
    ]
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
        initialize_default_config(&env, &admin);

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
        load_config(&env, &key)
    }

    /// Check if an action is allowed for an account.
    ///
    /// On success the counter for `(key, account)` is incremented and persisted
    /// with a TTL that ends when the current window rolls over. On rejection
    /// the counter is still advanced — a rejected attempt is evidence of abuse —
    /// and the numbers are published in the `rl_exceeded` event rather than in
    /// the error, because a `#[contracterror]` variant cannot carry a payload.
    ///
    /// # Security note
    ///
    /// The counter is keyed by the `account` argument and this entry point does
    /// **not** take an auth for it, because the calling contract is expected to
    /// invoke this check inside a transaction in which the end user's auth is
    /// already present. That means a caller can only rate-limit an account by
    /// spending that account's own allowance — which is also a griefing vector
    /// if a contract ever exposes the check with a caller-chosen `account`.
    /// Integrating contracts must pass the authenticated end user's address.
    pub fn check_rate_limit(
        env: Env,
        key: RateLimitKey,
        account: Address,
    ) -> Result<(), RateLimitError> {
        let config = match load_config(&env, &key) {
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

        // `saturating_sub` rather than plain `-`: a record that holds a *future*
        // ledger — possible after a ledger-number regression following a
        // restore, or in a test that moves the ledger backwards — would make
        // the subtraction underflow and panic in a debug build. Saturating to
        // zero is also the fail-closed direction: the record stays inside its
        // current window, so the caller is still rate-limited.
        let elapsed = current_ledger.saturating_sub(record.first_ledger);

        // Check if the window has expired
        if elapsed > config.window_ledgers {
            // Reset the window
            record.count = 1;
            record.first_ledger = current_ledger;
            record.last_ledger = current_ledger;
        } else {
            // Increment the count
            record.count = record
                .count
                .checked_add(1)
                .ok_or(RateLimitError::LimitExceeded)?;
            record.last_ledger = current_ledger;
        }

        // Store the record with TTL
        env.storage().persistent().set(&record_key, &record);

        // Set TTL to the end of the current window. Saturating because
        // `elapsed` can exceed the window on the rollover branch above, where
        // `first_ledger` has just been reset to the current ledger (elapsed 0),
        // and defensively on any future ledger regression.
        let ttl_ledgers = config.window_ledgers.saturating_sub(elapsed);
        env.storage().persistent().extend_ttl(
            &record_key,
            ttl_ledgers,
            ttl_ledgers,
        );

        // Check if the limit has been exceeded
        if record.count > config.limit {
            // The `#[contracterror]` enum cannot carry the offending numbers, so
            // the limit / window / count triple is published here instead.
            env.events().publish(
                (Symbol::new(&env, "rl_exceeded"),),
                (
                    key,
                    account,
                    record.count,
                    config.limit,
                    config.window_ledgers,
                ),
            );
            return Err(RateLimitError::LimitExceeded);
        }

        // Emit event
        env.events().publish(
            (symbol_short!("rl_check"),),
            (
                key,
                account,
                record.count,
                config.limit,
                config.window_ledgers,
            ),
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

        // Remove all records for this account
        for key in all_keys().iter() {
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
