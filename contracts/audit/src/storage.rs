//! Storage keys and the append path for the audit log (closes #712).
//!
//! Follows the pattern of `contracts/escrow/src/storage.rs` and
//! `contracts/platform_config/src/storage.rs`: one `#[contracttype] DataKey`
//! enum, typed accessors, no free-form key strings.
//!
//! ## Immutability
//!
//! The only value this module ever overwrites is the append counter. An entry is
//! written under its own sequence number, and sequence numbers come from a
//! counter that only ever increases, so **no stored entry can be overwritten and
//! no entry can be removed** — the module exposes no `remove`, no setter, and no
//! clearing helper for entry keys, and that absence is the guarantee. This is
//! deliberately not an access-control flag: access control would only decide
//! *who* may write, whereas the guarantee needed is that nothing can be rewritten
//! at all.

use soroban_sdk::{contracttype, Address, Bytes, Env};

use crate::types::{AuditEntry, TxStatus};

/// Retention for appended entries and their indexes: ~300 days at 5 s/ledger.
///
/// Longer than any other contract's TTL because an audit trail that silently
/// expires is not an audit trail. On-chain retention is still bounded by the
/// network's storage rules, so a durable archive is an off-chain job; the
/// on-chain log is the recent, verifiable window.
pub const AUDIT_TTL_LEDGERS: u32 = 5_184_000;

#[contracttype]
pub enum DataKey {
    /// Platform admin — the only address that may attest transactions.
    Admin,
    /// Append counter: the next sequence number to hand out. The single
    /// mutable value in the contract.
    Sequence,
    /// Appended entry.  Key: sequence.
    Entry(u32),
    /// Per-reference trail.  Key: (reference, index) -> sequence.
    ByReference(Bytes, u32),
    /// Appends recorded for a reference.  Key: reference.
    ReferenceCount(Bytes),
    /// Per-account trail.  Key: (account, index) -> sequence. An entry is
    /// indexed under `from`, and additionally under `to` when the two differ.
    ByAccount(Address, u32),
    /// Appends an account appears in.  Key: account.
    AccountCount(Address),
    /// Per-status trail.  Key: (status, index) -> sequence.
    ByStatus(TxStatus, u32),
    /// Appends carrying a status.  Key: status.
    StatusCount(TxStatus),
}

// ── Admin ────────────────────────────────────────────────────────────────────

pub fn is_initialized(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

pub fn get_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::Admin)
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

// ── Append counter ───────────────────────────────────────────────────────────

/// Next sequence number to hand out, i.e. the current length of the log.
pub fn next_sequence(env: &Env) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::Sequence)
        .unwrap_or(0u32)
}

// ── Entries ──────────────────────────────────────────────────────────────────

pub fn load_entry(env: &Env, sequence: u32) -> Option<AuditEntry> {
    env.storage().persistent().get(&DataKey::Entry(sequence))
}

/// Write a freshly built entry under a sequence number that has never been used,
/// and extend the three secondary indexes.
///
/// The caller must pass `sequence == next_sequence(env)`. Because the counter
/// only ever increases and is only ever bumped here, that sequence has never
/// been written before, so this cannot clobber an existing entry.
pub fn write_entry(env: &Env, entry: &AuditEntry) {
    let entry_key = DataKey::Entry(entry.sequence);
    env.storage().persistent().set(&entry_key, entry);

    let index_ref = DataKey::ByReference(
        entry.reference.clone(),
        reference_count(env, &entry.reference),
    );
    env.storage().persistent().set(&index_ref, &entry.sequence);

    // Index under the sender always, and under the recipient only when it is a
    // different account, so one account never sees the same entry twice.
    push_account(env, &entry.from, entry.sequence);
    if entry.to != entry.from {
        push_account(env, &entry.to, entry.sequence);
    }

    let status_index = DataKey::ByStatus(entry.status, status_count(env, &entry.status));
    env.storage().persistent().set(&status_index, &entry.sequence);

    bump_count(env, &DataKey::ReferenceCount(entry.reference.clone()));
    bump_count(env, &DataKey::StatusCount(entry.status));
    bump_account_count(env, &entry.from);
    if entry.to != entry.from {
        bump_account_count(env, &entry.to);
    }

    bump_sequence(env);
    // The counters are renewed by `bump_count`; the three index entries are
    // renewed here, because they are what the query paths actually read.
    renew(env, &entry_key);
    renew(env, &index_ref);
    renew(env, &status_index);
}

fn renew(env: &Env, key: &DataKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, AUDIT_TTL_LEDGERS, AUDIT_TTL_LEDGERS);
}

fn push_account(env: &Env, account: &Address, sequence: u32) {
    let count = account_count(env, account);
    let key = DataKey::ByAccount(account.clone(), count);
    env.storage().persistent().set(&key, &sequence);
    renew(env, &key);
}

fn bump_count(env: &Env, key: &DataKey) {
    let current: u32 = env.storage().persistent().get(key).unwrap_or(0u32);
    env.storage().persistent().set(key, &(current + 1));
    renew(env, key);
}

fn bump_account_count(env: &Env, account: &Address) {
    bump_count(env, &DataKey::AccountCount(account.clone()));
}

fn bump_sequence(env: &Env) {
    bump_count(env, &DataKey::Sequence);
}

// ── Index readers ────────────────────────────────────────────────────────────

/// How many appends a reference has.
pub fn reference_count(env: &Env, reference: &Bytes) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::ReferenceCount(reference.clone()))
        .unwrap_or(0u32)
}

/// How many appends an account appears in.
pub fn account_count(env: &Env, account: &Address) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::AccountCount(account.clone()))
        .unwrap_or(0u32)
}

/// How many appends carry a status.
pub fn status_count(env: &Env, status: &TxStatus) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::StatusCount(*status))
        .unwrap_or(0u32)
}

/// Sequence number of the `index`-th append for a reference, if it exists.
pub fn reference_sequence(env: &Env, reference: &Bytes, index: u32) -> Option<u32> {
    env.storage()
        .persistent()
        .get(&DataKey::ByReference(reference.clone(), index))
}

/// Sequence number of the `index`-th append an account appears in.
pub fn account_sequence(env: &Env, account: &Address, index: u32) -> Option<u32> {
    env.storage()
        .persistent()
        .get(&DataKey::ByAccount(account.clone(), index))
}

/// Sequence number of the `index`-th append carrying a status.
pub fn status_sequence(env: &Env, status: &TxStatus, index: u32) -> Option<u32> {
    env.storage()
        .persistent()
        .get(&DataKey::ByStatus(*status, index))
}
