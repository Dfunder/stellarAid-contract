use soroban_sdk::{contracttype, Address, Bytes, Env, Vec};

use crate::errors::EscrowError;

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommissionStatus {
    Locked = 0,
    Released = 1,
    Refunded = 2,
    Disputed = 3,
    Expired = 4,
    /// Settled early under a commission cancellation (#605).
    Cancelled = 5,
    /// Partially released — one or more milestones paid, remainder held.
    PartiallyReleased = 6,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct EscrowRecord {
    pub commission_id: Bytes,
    pub client: Address,
    pub artist: Address,
    pub amount: i128,
    pub fee_bps: u32,
    pub status: CommissionStatus,
    pub created_ledger: u32,
    /// Total amount already released via milestone partial releases (#601).
    pub released_amount: i128,
}

// ---------------------------------------------------------------------------
// Milestone release types — closes #601
// ---------------------------------------------------------------------------

/// A single milestone within an escrow, supporting partial release (#601).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowMilestone {
    pub milestone_id: Bytes,
    /// Amount in USDC smallest-denomination units.
    pub amount: i128,
    pub status: MilestoneReleaseStatus,
    /// Ledger after which the milestone auto-releases if not yet released.
    pub auto_release_ledger: u32,
}

/// Lifecycle state of a single milestone for release purposes.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MilestoneReleaseStatus {
    /// Awaiting client approval.
    Pending = 0,
    /// Client approved; funds transferable.
    Approved = 1,
    /// Funds already transferred to artist.
    Released = 2,
    /// Auto-released because deadline passed.
    AutoReleased = 3,
}

#[contracttype]
pub enum DataKey {
    Escrow(Bytes),
    /// Re-entrancy guard flag (#484, #587).
    ReentrancyLock,
    /// Configurable dispute-period TTL extension in ledgers (#586).
    DisputeTtlLedgers,
    /// List of milestones for a commission (#601).
    Milestones(Bytes),
}

pub fn escrow_exists(env: &Env, id: &Bytes) -> bool {
    env.storage().persistent().has(&DataKey::Escrow(id.clone()))
}
pub fn get_escrow(env: &Env, id: &Bytes) -> EscrowRecord {
    env.storage().persistent().get(&DataKey::Escrow(id.clone())).unwrap()
}
pub fn save_escrow(env: &Env, r: &EscrowRecord) {
    env.storage().persistent().set(&DataKey::Escrow(r.commission_id.clone()), r);
}

// ── Milestone helpers — closes #601 ────────────────────────────────────────

pub fn get_milestones(env: &Env, commission_id: &Bytes) -> Vec<EscrowMilestone> {
    env.storage()
        .persistent()
        .get(&DataKey::Milestones(commission_id.clone()))
        .unwrap_or(Vec::new(env))
}

pub fn save_milestones(env: &Env, commission_id: &Bytes, milestones: &Vec<EscrowMilestone>) {
    env.storage()
        .persistent()
        .set(&DataKey::Milestones(commission_id.clone()), milestones);
}

// ── Re-entrancy lock helpers (#484) ────────────────────────────────────────

/// Returns `true` if a re-entrancy lock is currently held.
pub fn is_locked(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::ReentrancyLock)
}

/// Acquire the re-entrancy lock.
pub fn set_locked(env: &Env) {
    env.storage().instance().set(&DataKey::ReentrancyLock, &true);
}

/// Release the re-entrancy lock.
pub fn clear_locked(env: &Env) {
    env.storage().instance().remove(&DataKey::ReentrancyLock);
}

/// Runs `f` under the re-entrancy guard (#587).
///
/// Rejects any call that arrives while a guarded entry point is still
/// executing, and always releases the lock once `f` returns — including on
/// the error path. If `f` panics or aborts, the transaction reverts and the
/// instance storage (and therefore the lock) is discarded with it.
pub fn with_reentrancy_guard<F, T>(env: &Env, f: F) -> Result<T, EscrowError>
where
    F: FnOnce() -> Result<T, EscrowError>,
{
    if is_locked(env) {
        return Err(EscrowError::Reentrant);
    }
    set_locked(env);
    let result = f();
    clear_locked(env);
    result
}

// ── Dispute-period TTL configuration (#586) ────────────────────────────────

/// Default dispute-period TTL: ~60 days at 6s/ledger. Roughly twice the base
/// escrow TTL so an active dispute never races record expiration.
pub const DEFAULT_DISPUTE_TTL_LEDGERS: u32 = 864_000;

/// Returns the configured dispute-period TTL, falling back to the default.
pub fn get_dispute_ttl_ledgers(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::DisputeTtlLedgers)
        .unwrap_or(DEFAULT_DISPUTE_TTL_LEDGERS)
}

/// Persists the dispute-period TTL.
pub fn set_dispute_ttl_ledgers(env: &Env, ledgers: u32) {
    env.storage().instance().set(&DataKey::DisputeTtlLedgers, &ledgers);
}