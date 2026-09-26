//! Rate Limiter Types
//!
//! Type definitions for the rate limiter module.

use soroban_sdk::Address;

/// Different types of rate limits that can be configured
#[derive(Clone, Debug, Eq, PartialEq, Copy)]
#[soroban_sdk::contracttype]
pub enum RateLimitKey {
    /// Rate limit for commission creation per artist
    CommissionsPerArtist,
    /// Rate limit for dispute filing per user
    DisputesPerUser,
    /// Rate limit for escrow creation per user
    EscrowsPerUser,
}

/// Configuration for a specific rate limit
#[derive(Clone, Debug, Eq, PartialEq)]
#[soroban_sdk::contracttype]
pub struct RateLimitConfig {
    /// Maximum number of actions allowed in the window
    pub limit: u32,
    /// Duration of the rate limit window in ledgers
    pub window_ledgers: u32,
}

/// Record tracking the usage of a rate limit
#[derive(Clone, Debug, Eq, PartialEq)]
#[soroban_sdk::contracttype]
pub struct RateLimitRecord {
    /// Type of rate limit
    pub key: RateLimitKey,
    /// Account being limited
    pub account: Address,
    /// Current count of actions in this window
    pub count: u32,
    /// Ledger number when the current window started
    pub first_ledger: u32,
    /// Ledger number of the last action
    pub last_ledger: u32,
}

impl RateLimitRecord {
    /// Create a storage key for this record
    pub fn key(key: RateLimitKey, account: Address) -> DataKey {
        DataKey::RateLimitRecord(key, account)
    }
}

/// Storage keys for the rate limiter
#[derive(Clone)]
#[soroban_sdk::contracttype]
pub enum DataKey {
    /// Admin address
    Admin,
    /// Rate limit configurations
    RateLimitConfig,
    /// Rate limit record for a specific key and account
    RateLimitRecord(RateLimitKey, Address),
}
