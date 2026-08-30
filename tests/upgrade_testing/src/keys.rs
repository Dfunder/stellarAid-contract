//! Storage keys shared across mock contract versions.
//!
//! ABI rule enforced by this framework: `#[contracttype]` variants and their
//! values are **permanent** across versions. v2 only *adds* keys.

use soroban_sdk::{contracttype, Address, Bytes};

#[contracttype]
pub enum DataKey {
    /// Instance: contract admin.
    Admin,
    /// Instance: storage schema version written by the last migration.
    Schema,
    /// Persistent: `Vec<Address>` of roster members for a roster id.
    Roster(Bytes),
    /// Persistent: migration-completed marker for a roster id (v2 only).
    Relay(Bytes),
    /// Persistent: per-member share (bps of 10_000) after migration (v2 only).
    Member(Bytes, Address),
}

/// Default share denominator for migration-derived shares (10_000 bps = 100%).
pub const SHARE_BASE: u32 = 10_000;