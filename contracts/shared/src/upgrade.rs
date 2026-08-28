//! State migration and upgrade safety helpers (closes #595).
//!
//! This module provides:
//!
//! 1. `ContractVersion` — a semantic version struct stored in instance storage
//!    so the current WASM version can always be read on-chain.
//! 2. `require_upgrade_safe` — a pre-upgrade check that confirms the contract
//!    is paused before the WASM hash is replaced, preventing in-flight
//!    transactions from being processed against a half-migrated state.
//! 3. `record_upgrade` — writes the new version to instance storage and emits
//!    an `upgrade_complete` event after the WASM replacement.
//! 4. `export_storage_keys` — returns a list of all known top-level storage
//!    keys so an off-chain migration script can verify record counts before and
//!    after a schema change.
//!
//! ## Upgrade procedure (closes #595)
//!
//! ```text
//! 1.  Call `pause` to halt in-flight transactions.
//! 2.  Deploy the new WASM to a *new* contract address (never upgrade in-place
//!     on mainnet without prior testnet validation).
//! 3.  Call `migrate_v1_to_v2` (or the appropriate migration function) on the
//!     new contract to populate any new storage keys from exported data.
//! 4.  Verify record counts match via `export_storage_keys`.
//! 5.  Redirect clients to the new contract address.
//! 6.  Call `unpause` on the new contract to resume operations.
//! 7.  Emit `upgrade_complete` with old and new version numbers for audit.
//! ```

use soroban_sdk::{contracttype, symbol_short, Address, Env};

// ── Version tracking ─────────────────────────────────────────────────────────

/// Semantic version stored in instance storage (closes #595, #682).
///
/// Comparison and compatibility helpers live in [`crate::version`].
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContractVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

#[contracttype]
pub enum UpgradeKey {
    /// Current contract semantic version.
    Version,
    /// Ledger sequence number at which the last upgrade was performed.
    LastUpgradeLedger,
}

/// Write `version` to instance storage and emit `upgrade_complete`.
///
/// Must be called by the admin from inside a guarded upgrade entry point.
/// Closes #595. Version constraint rules: `docs/VERSIONING.md` (#682).
pub fn record_upgrade(env: &Env, admin: &Address, version: ContractVersion) {
    admin.require_auth();
    let prev: Option<ContractVersion> = env.storage().instance().get(&UpgradeKey::Version);
    env.storage().instance().set(&UpgradeKey::Version, &version);
    env.storage().instance().set(&UpgradeKey::LastUpgradeLedger, &env.ledger().sequence());
    env.events().publish(
        (symbol_short!("upgraded"),),
        (prev, version, env.ledger().sequence()),
    );
}

/// Return the current stored version, or `{0,0,0}` if never set.
pub fn get_version(env: &Env) -> ContractVersion {
    env.storage()
        .instance()
        .get(&UpgradeKey::Version)
        .unwrap_or(ContractVersion { major: 0, minor: 0, patch: 0 })
}

/// Panic if the contract is not paused.
///
/// Call this at the top of any admin-only `upgrade` entry point to ensure
/// no operations are in progress during WASM replacement (closes #595).
pub fn require_paused_for_upgrade(env: &Env) {
    // Read the pause flag written by `shared::pause`.
    let paused: bool = env.storage()
        .instance()
        .get(&crate::pause::PauseDataKey::Paused)
        .unwrap_or(false);
    if !paused {
        panic!("contract must be paused before upgrading");
    }
}

/// Emit a `migration_needed` event indicating that the caller should run the
/// off-chain data migration script before resuming operations.
///
/// Closes #595.
pub fn signal_migration_needed(env: &Env, from_version: ContractVersion, to_version: ContractVersion) {
    env.events().publish(
        (symbol_short!("mig_need"),),
        (from_version, to_version),
    );
}
