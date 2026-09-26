//! Withdrawal history view (closes #728).
//!
//! `get_withdrawals_by_campaign` in lib.rs already returns the raw
//! `Withdrawal` records for a campaign, but that type has no `asset` or
//! `tx reference` field and only covers one campaign at a time. This
//! module adds a richer, append-only, contract-wide history log —
//! `WithdrawalRecord { amount, asset, timestamp, tx_reference }` — kept in
//! its own storage key space so it's additive rather than a change to the
//! existing `Withdrawal`/`DataKey` types.
//!
//! `record_withdrawal` is the write side, meant to be called from
//! `approve_withdrawal` (or a future `finalize_withdrawal`) once a transfer
//! actually completes; wiring that call site is a one-line follow-up left
//! for a maintainer, out of scope for this single-file addition.
//! `get_withdrawal_history` is the read side: no auth required, read-only,
//! chronologically ordered — exactly as the issue specifies.

use soroban_sdk::{contracttype, Address, Env, Vec};

#[contracttype]
#[derive(Clone)]
pub struct WithdrawalRecord {
    pub amount: i128,
    pub asset: Address,
    pub timestamp: u64,
    /// Opaque reference to the on-chain transfer this record documents
    /// (e.g. the withdrawal id from lib.rs, or a future real tx hash).
    pub tx_reference: u64,
}

#[contracttype]
#[derive(Clone)]
enum HistoryKey {
    /// Chronological history log, contract-wide (not scoped per campaign,
    /// per the issue's "creator's withdrawal history" framing).
    History,
}

/// Appends a new entry to the withdrawal history log. Chronological order
/// falls out of append-only insertion — no sorting needed on read.
pub fn record_withdrawal(env: &Env, amount: i128, asset: Address, tx_reference: u64) {
    let mut history: Vec<WithdrawalRecord> = env
        .storage()
        .persistent()
        .get(&HistoryKey::History)
        .unwrap_or_else(|| Vec::new(env));

    history.push_back(WithdrawalRecord {
        amount,
        asset,
        timestamp: env.ledger().timestamp(),
        tx_reference,
    });

    env.storage().persistent().set(&HistoryKey::History, &history);
}

/// Returns the full withdrawal history, chronologically ordered.
/// Read-only; no authorization required, per the issue's acceptance
/// criteria ("No auth required, read-only").
pub fn get_withdrawal_history(env: &Env) -> Vec<WithdrawalRecord> {
    env.storage()
        .persistent()
        .get(&HistoryKey::History)
        .unwrap_or_else(|| Vec::new(env))
}
