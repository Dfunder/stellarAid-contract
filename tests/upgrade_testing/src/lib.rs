//! Smart Contract Upgrade Testing Framework (closes #689).
//!
//! In-memory upgrade simulator and regression suite for the workspace
//! contracts. It models the real Soroban upgrade flow:
//!
//! 1. **Deploy** the current (v1) WASM behaviour as a mock contract and seed
//!    it with live-shaped data.
//! 2. **Snapshot** the contract's instance and persistent storage.
//! 3. **Migrate** the snapshot with the upgrade's (real or simulated) migration
//!    logic — schema-compatible swaps keep the snapshot as-is; schema changes
//!    transform it (this is where an on-chain `migrate_*` entry point is
//!    exercised).
//! 4. **Restore** into a v2 contract (the deployed new WASM) and **verify**:
//!    state preserved, backward compatibility holds, migration is idempotent.
//! 5. Run the **regression suite** against the upgraded instance.
//!
//! Companion documentation: [`docs/UPGRADE_TESTING.md`](../../docs/UPGRADE_TESTING.md).
//!
//! This crate's entire purpose is executing in `cargo test`, and its simulator
//! relies on Soroban test-utils (`register_contract`, `as_contract`), so it is
//! compiled only under the `test` cfg.

#![cfg(test)]

pub mod keys;
pub mod regression;
pub mod simulator;
pub mod v1;
pub mod v2;

#[cfg(test)]
mod scenarios;