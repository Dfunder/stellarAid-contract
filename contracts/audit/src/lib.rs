//! Transaction History and Audit Log Contract
//!
//! Closes #712 – Create Transaction History and Audit Log System.
//!
//! ## Immutability — how it is actually achieved
//!
//! In Soroban the way to get an append-only store is structural, not a flag:
//!
//! * Entries are written **once**, under a sequence number drawn from a counter
//!   that only ever increases. A sequence number is therefore never reused, so
//!   an existing entry can never be overwritten.
//! * **There is no `update_entry`, no `delete_entry`, and no `clear`.** Their
//!     absence *is* the guarantee: there is no code path, authenticated or not,
//!   that can mutate or remove a stored entry. A status change is not an update
//!   — it is a further append under the same `reference`, so the earlier state
//!   is still readable and the whole trail is preserved.
//! * Access control decides *who may append*, never *whether a stored entry may
//!   change*. Those are different guarantees and only the second one makes this
//!   an audit log.
//!
//! What the immutability guarantee does **not** cover is retention: entries live
//! in persistent storage with a bounded TTL (see
//! [`storage::AUDIT_TTL_LEDGERS`]). That is a property of the network's storage
//! model, not something a contract can override.
//!
//! ## Query bounds
//!
//! Two constants bound every read, so no caller can force an unbounded read:
//!
//! | Constant          | Value | Meaning                                     |
//! |-------------------|-------|---------------------------------------------|
//! | `MAX_PAGE_SIZE`   | 50    | Most entries one query may return            |
//! | `MAX_QUERY_SCAN`  | 200   | Most stored entries one query may *visit*    |
//!
//! The account, status, and reference queries walk an index and therefore stay
//! proportional to the page size. The ledger-range query has no index, so it
//! scans forward from a caller-supplied `start_sequence` and stops after
//! `MAX_QUERY_SCAN` entries; when it does, the result is flagged
//! `truncated` and carries a `next_sequence` cursor so the caller can continue.
//! Ignoring a `truncated` flag means reporting on a partial window.
//!
//! ## What is deliberately not recorded
//!
//! No free text, no memos, no names, no identifiers of any kind. `action` is a
//! closed [`AuditAction`] enum, parties are contract addresses, and amounts are
//! the `i128` smallest-unit integers the rest of the workspace uses. There is no
//! float arithmetic anywhere in this crate, and no place for PII to land.

#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Bytes, Env, Vec};

pub mod errors;
pub mod storage;
pub mod types;

#[cfg(test)]
mod test;

use errors::AuditError;
use types::{AuditAction, AuditEntry, AuditExport, AuditPage, TxStatus};

/// Re-exported so the immutability claim can be audited from the outside: this
/// is the complete set of storage keys, and not one of them has a remove, a
/// setter, or a clear path.
pub use storage::DataKey as StorageKey;

include!("../../semver_types.rs");

/// Most entries a single query may return (#712).
pub const MAX_PAGE_SIZE: u32 = 50;
/// Most stored entries a single query may visit (#712). Only the ledger-range
/// scan and the export can hit this; the index-backed queries walk a page.
pub const MAX_QUERY_SCAN: u32 = 200;

#[contract]
pub struct AuditLog;

fn require_admin(env: &Env) -> Result<Address, AuditError> {
    let admin = storage::get_admin(env).ok_or(AuditError::NotInitialized)?;
    admin.require_auth();
    Ok(admin)
}

/// Validate and build an entry without writing it. Split out so both append
/// paths share exactly one set of checks.
fn build_entry(
    env: &Env,
    reference: &Bytes,
    action: AuditAction,
    from: &Address,
    to: &Address,
    amount: i128,
    token: &Option<Address>,
    status: TxStatus,
) -> Result<AuditEntry, AuditError> {
    if amount < 0 {
        return Err(AuditError::InvalidAmount);
    }
    // A value transfer names its asset; an activity marker names none. The first
    // branch is reachable today (`record_transaction` can be handed the activity
    // label); the second guards any future caller of this constructor, because
    // the two entry kinds must not be interchanged.
    if action == AuditAction::UserActivity && token.is_some() {
        return Err(AuditError::InvalidAction);
    }
    if action != AuditAction::UserActivity && token.is_none() {
        return Err(AuditError::InvalidAction);
    }
    Ok(AuditEntry {
        sequence: storage::next_sequence(env),
        reference: reference.clone(),
        action,
        from: from.clone(),
        to: to.clone(),
        amount,
        token: token.clone(),
        status,
        ledger: env.ledger().sequence(),
        timestamp: env.ledger().timestamp(),
    })
}

fn check_page(page_size: u32) -> Result<(), AuditError> {
    if page_size == 0 || page_size > MAX_PAGE_SIZE {
        return Err(AuditError::InvalidPageSize);
    }
    Ok(())
}

fn page_bounds(total: u32, page: u32, page_size: u32) -> (u32, u32) {
    let start = page.saturating_mul(page_size);
    if start >= total {
        // Clamp to the end rather than to zero, so `has_more` reads
        // `end < total` and correctly reports that nothing follows.
        return (total, total);
    }
    let end = core::cmp::min(start.saturating_add(page_size), total);
    (start, end)
}

/// Slice `page` out of an already-materialised match list.
fn slice_page(
    env: &Env,
    matches: Vec<AuditEntry>,
    page: u32,
    page_size: u32,
    truncated: bool,
    next_sequence: Option<u32>,
) -> AuditPage {
    let total = matches.len();
    let (start, end) = page_bounds(total, page, page_size);
    let mut entries: Vec<AuditEntry> = Vec::new(env);
    let mut i = start;
    while i < end {
        entries.push_back(matches.get(i).unwrap());
        i += 1;
    }
    AuditPage {
        entries,
        page,
        page_size,
        total,
        has_more: end < total,
        truncated,
        next_sequence,
    }
}

/// Walk an index of sequence numbers and load the entries it points at.
fn page_from_index(
    env: &Env,
    total: u32,
    page: u32,
    page_size: u32,
    lookup: impl Fn(u32) -> Option<u32>,
) -> AuditPage {
    let (start, end) = page_bounds(total, page, page_size);
    let mut entries: Vec<AuditEntry> = Vec::new(env);
    let mut i = start;
    while i < end {
        if let Some(sequence) = lookup(i) {
            if let Some(entry) = storage::load_entry(env, sequence) {
                entries.push_back(entry);
            }
        }
        i += 1;
    }
    AuditPage {
        entries,
        page,
        page_size,
        total,
        has_more: end < total,
        truncated: false,
        next_sequence: if end < total { Some(end) } else { None },
    }
}

/// Walk stored entries from `start_sequence` looking for a ledger window.
///
/// Stops after [`MAX_QUERY_SCAN`] visited entries. Returns the matches plus
/// whether the walk was cut short and where to resume.
fn scan_ledger_range(
    env: &Env,
    from_ledger: u32,
    to_ledger: u32,
    start_sequence: u32,
) -> (Vec<AuditEntry>, bool, Option<u32>) {
    let end_sequence = storage::next_sequence(env);
    let mut matches: Vec<AuditEntry> = Vec::new(env);
    let mut sequence = start_sequence;
    let mut visited: u32 = 0;
    while sequence < end_sequence && visited < MAX_QUERY_SCAN {
        if let Some(entry) = storage::load_entry(env, sequence) {
            if entry.ledger >= from_ledger && entry.ledger <= to_ledger {
                matches.push_back(entry);
            }
        }
        sequence += 1;
        visited += 1;
    }
    let truncated = sequence < end_sequence;
    let next = if truncated { Some(sequence) } else { None };
    (matches, truncated, next)
}

#[contractimpl]
impl AuditLog {
    /// One-shot bootstrap. The admin is the platform oracle that attests
    /// transactions; users attest only their own activity.
    pub fn initialize(env: Env, admin: Address) -> Result<(), AuditError> {
        admin.require_auth();
        if storage::is_initialized(&env) {
            return Err(AuditError::AlreadyInitialized);
        }
        storage::set_admin(&env, &admin);
        env.events().publish((symbol_short!("aud_init"),), admin);
        Ok(())
    }

    impl_semver_queries!();

    /// Append a transaction-history entry. Admin only.
    ///
    /// Never rewrites anything: a status change is a new entry under the same
    /// `reference`, so `get_transaction` returns the newest state while
    /// `get_transaction_history` returns the whole trail.
    pub fn record_transaction(
        env: Env,
        reference: Bytes,
        action: AuditAction,
        from: Address,
        to: Address,
        amount: i128,
        token: Address,
        status: TxStatus,
    ) -> Result<u32, AuditError> {
        require_admin(&env)?;
        let entry = build_entry(
            &env,
            &reference,
            action,
            &from,
            &to,
            amount,
            &Some(token),
            status,
        )?;
        storage::write_entry(&env, &entry);
        env.events().publish(
            (symbol_short!("aud_txn"),),
            (reference, entry.sequence, entry.action, entry.status, entry.amount),
        );
        Ok(entry.sequence)
    }

    /// Append a self-attested user activity entry.
    ///
    /// `actor.require_auth()` means an account can only ever log activity in its
    /// own name, so this path cannot be used to forge a record about someone
    /// else. The entry's kind, status, amount, and token are all fixed here and
    /// are not caller-supplied: an activity marker can never be shaped to look
    /// like a value settlement.
    pub fn log_activity(
        env: Env,
        actor: Address,
        reference: Bytes,
    ) -> Result<u32, AuditError> {
        actor.require_auth();
        let entry = build_entry(
            &env,
            &reference,
            AuditAction::UserActivity,
            &actor,
            &actor,
            0,
            &None,
            TxStatus::Activity,
        )?;
        storage::write_entry(&env, &entry);
        env.events().publish(
            (symbol_short!("aud_act"),),
            (reference, entry.sequence, actor),
        );
        Ok(entry.sequence)
    }

    // ── Reads ───────────────────────────────────────────────────────────────

    /// Total appends in the log. Also the next sequence number.
    pub fn get_entry_count(env: Env) -> u32 {
        storage::next_sequence(&env)
    }

    /// One entry by its sequence number.
    pub fn get_entry(env: Env, sequence: u32) -> Result<AuditEntry, AuditError> {
        storage::load_entry(&env, sequence).ok_or(AuditError::EntryNotFound)
    }

    /// The newest entry for a reference — i.e. its current state.
    pub fn get_transaction(env: Env, reference: Bytes) -> Result<AuditEntry, AuditError> {
        let count = storage::reference_count(&env, &reference);
        if count == 0 {
            return Err(AuditError::ReferenceNotFound);
        }
        storage::load_entry(&env, storage::reference_sequence(&env, &reference, count - 1).unwrap_or(0))
            .ok_or(AuditError::EntryNotFound)
    }

    /// The full status trail for a reference, oldest first.
    pub fn get_transaction_history(
        env: Env,
        reference: Bytes,
        page: u32,
        page_size: u32,
    ) -> Result<AuditPage, AuditError> {
        check_page(page_size)?;
        let total = storage::reference_count(&env, &reference);
        Ok(page_from_index(&env, total, page, page_size, |i| {
            storage::reference_sequence(&env, &reference, i)
        }))
    }

    /// Every append an account appears in, as sender or recipient.
    pub fn get_by_account(
        env: Env,
        account: Address,
        page: u32,
        page_size: u32,
    ) -> Result<AuditPage, AuditError> {
        check_page(page_size)?;
        let total = storage::account_count(&env, &account);
        Ok(page_from_index(&env, total, page, page_size, |i| {
            storage::account_sequence(&env, &account, i)
        }))
    }

    /// Every append carrying a given status.
    pub fn get_by_status(
        env: Env,
        status: TxStatus,
        page: u32,
        page_size: u32,
    ) -> Result<AuditPage, AuditError> {
        check_page(page_size)?;
        let total = storage::status_count(&env, &status);
        Ok(page_from_index(&env, total, page, page_size, |i| {
            storage::status_sequence(&env, &status, i)
        }))
    }

    /// Every append whose ledger falls inside `[from_ledger, to_ledger]`.
    ///
    /// There is no index on the ledger, so the walk is bounded by
    /// [`MAX_QUERY_SCAN`] entries starting at `start_sequence`. When the bound
    /// is hit the page comes back `truncated` with a `next_sequence` cursor;
    /// feed that back in to continue. A caller that ignores the flag is
    /// reporting on a partial window.
    pub fn get_by_ledger_range(
        env: Env,
        from_ledger: u32,
        to_ledger: u32,
        start_sequence: u32,
        page: u32,
        page_size: u32,
    ) -> Result<AuditPage, AuditError> {
        check_page(page_size)?;
        if from_ledger > to_ledger {
            return Err(AuditError::InvalidLedgerRange);
        }
        let (matches, truncated, next) =
            scan_ledger_range(&env, from_ledger, to_ledger, start_sequence);
        Ok(slice_page(&env, matches, page, page_size, truncated, next))
    }

    /// Export a ledger window for an audit report.
    ///
    /// Same bounded walk as `get_by_ledger_range`, but shaped for reporting:
    /// it reports the window it was asked for and an explicit `truncated` flag,
    /// so a report can never be mistaken for a complete one.
    pub fn export_audit_log(
        env: Env,
        from_ledger: u32,
        to_ledger: u32,
        start_sequence: u32,
        page: u32,
        page_size: u32,
    ) -> Result<AuditExport, AuditError> {
        check_page(page_size)?;
        if from_ledger > to_ledger {
            return Err(AuditError::InvalidLedgerRange);
        }
        let (matches, truncated, _) =
            scan_ledger_range(&env, from_ledger, to_ledger, start_sequence);
        let total = matches.len();
        let (start, end) = page_bounds(total, page, page_size);
        let mut entries: Vec<AuditEntry> = Vec::new(&env);
        let mut i = start;
        while i < end {
            entries.push_back(matches.get(i).unwrap());
            i += 1;
        }
        Ok(AuditExport {
            from_ledger,
            to_ledger,
            entries,
            total,
            truncated,
        })
    }

    // ── Health monitoring (#678) and gradual rollout (#684) ──────────────
    pub fn health_check(env: Env) -> shared::health::HealthReport {
        let report = shared::health::health_check(&env);
        if report.anomaly {
            shared::rollout::maybe_auto_rollback(&env);
        }
        report
    }
    pub fn get_health_metrics(env: Env) -> shared::health::HealthMetrics {
        shared::health::get_metrics(&env)
    }
    pub fn get_sla_targets(env: Env) -> shared::health::SlaTargets {
        let _ = env;
        shared::health::sla_targets()
    }
    pub fn set_alert_config(env: Env, admin: Address, config: shared::health::AlertConfig) {
        admin.require_auth();
        shared::health::set_alert_config(&env, config);
    }
    pub fn get_alert_config(env: Env) -> shared::health::AlertConfig {
        shared::health::get_alert_config(&env)
    }
    pub fn detect_anomaly(env: Env) -> bool {
        shared::health::detect_anomaly(&env)
    }
    pub fn report_ok(env: Env, admin: Address) {
        admin.require_auth();
        shared::health::record_ok(&env);
    }
    pub fn report_error(env: Env, admin: Address) {
        admin.require_auth();
        shared::health::record_error(&env);
    }
    pub fn set_feature_flag(env: Env, admin: Address, flag: soroban_sdk::Symbol, enabled: bool) {
        admin.require_auth();
        shared::rollout::set_feature_flag(&env, &flag, enabled);
    }
    pub fn is_feature_enabled(env: Env, flag: soroban_sdk::Symbol) -> bool {
        shared::rollout::is_feature_enabled(&env, &flag)
    }
    pub fn set_canary_deployment(env: Env, admin: Address, canary: Address, stable: Address, canary_bps: u32) {
        admin.require_auth();
        shared::rollout::set_canary_deployment(&env, canary, stable, canary_bps);
    }
    pub fn route_to_canary(env: Env, caller: Address) -> bool {
        shared::rollout::route_to_canary(&env, &caller)
    }
    pub fn get_rollout_state(env: Env) -> shared::rollout::RolloutState {
        shared::rollout::get_state(&env)
    }
    pub fn set_rollback_trigger(env: Env, admin: Address, error_bps: u32) {
        admin.require_auth();
        shared::rollout::set_rollback_trigger(&env, error_bps);
    }
    pub fn should_rollback(env: Env) -> bool {
        shared::rollout::should_rollback(&env)
    }
    pub fn trigger_rollback(env: Env, admin: Address) {
        admin.require_auth();
        shared::rollout::trigger_rollback(&env, &admin);
    }
}
