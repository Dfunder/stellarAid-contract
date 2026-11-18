//! Emergency pause and time-locked recovery (closes #594, #711).
//!
//! ## Pause
//!
//! [`pause`] sets the flag, [`is_paused`] reads it, and [`require_not_paused`]
//! is the guard every mutating entry point calls. Consuming contracts
//! (escrow, campaign, donation, withdrawal, revenue_sharing) all call
//! `pause::pause(&env, &admin)` and `pause::unpause(&env, &admin)` with the
//! same two arguments, so the existing [`pause`] / [`unpause`] signatures are
//! frozen and must not change.
//!
//! ## Time-locked recovery (#711)
//!
//! An immediate `unpause` means a single compromised — or simply mistaken —
//! admin keypress undoes an emergency stop. [`schedule_recovery`] arms a
//! time-lock instead: it records the ledger at which recovery matures, and
//! [`unpause`] then *refuses* to run before that ledger. The chosen flow is:
//!
//! 1. `pause(&env, &admin)` — stop the contract.
//! 2. `schedule_recovery(&env, &admin, delay_ledgers)` — declare the intent to
//!    resume. Returns the ledger at which recovery matures and emits
//!    `recovery_scheduled`. Further pauses clear the pending lock.
//! 3. Wait for the lock to mature. Monitors can watch `recovery_eta`.
//! 4. `unpause(&env, &admin)` (or [`try_unpause`]) resumes and emits both
//!    `recovery_completed` and the original `contract_unpaused`.
//!
//! A contract that has never scheduled a recovery has no lock, so
//! [`unpause`] keeps behaving exactly as it did before #711 and the five
//! consumer contracts need no change. [`cancel_recovery`] disarms a lock that
//! was armed in error without unpausing anything.
//!
//! ## Typed errors
//!
//! [`PauseError`] is the typed failure mode, and [`require_not_paused_typed`] /
//! [`try_unpause`] return it instead of aborting. [`require_not_paused`] and
//! [`unpause`] keep their historical infallible signatures and abort with the
//! typed error's description, which is why the guard is still safe for the
//! five consumers.
//!
//! ## Known limitation
//!
//! The five consumer contracts have not been migrated to
//! [`require_not_paused_typed`] yet — that is deliberately **not** done in the
//! #711 change, because those contracts are out of scope for it. They still
//! see an abort (a `HostError`) instead of a `PauseError` they can inspect.
//! Adopting the typed error is a follow-up that touches `escrow`,
//! `campaign`, `donation`, `withdrawal` and `revenue_sharing`.
//!
//! [`shared::rollout::trigger_rollback`] also sets the pause flag directly; it
//! deliberately does not arm or clear a recovery lock.

use soroban_sdk::{contracterror, contracttype, symbol_short, Address, Env, Symbol};

/// Default time-lock for recovering from a pause: ~24 h at 5 s/ledger.
pub const DEFAULT_RECOVERY_DELAY_LEDGERS: u32 = 17_280;
/// Longest time-lock an admin may arm: ~7 days at 5 s/ledger. A longer pause
/// is an incident response, not a recovery, and should be handled by
/// escalating rather than by setting a number nobody will wait out.
pub const MAX_RECOVERY_DELAY_LEDGERS: u32 = 120_960;

/// Typed failures of the pause mechanism (#711).
///
/// The matching entries in the unified catalogue
/// (`shared::errors::SharedErrorCode`) are `ContractPaused = 6`,
/// `RecoveryPending = 15`, `RecoveryNotScheduled = 16`,
/// `InvalidRecoveryDelay = 17` and `ContractNotPaused = 18`.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PauseError {
    /// The contract is paused; the operation is not permitted.
    ContractPaused = 1,
    /// A recovery is scheduled but has not matured yet; the time-lock holds.
    RecoveryPending = 2,
    /// No recovery is scheduled, so there is nothing to cancel.
    RecoveryNotScheduled = 3,
    /// The requested delay is zero or beyond [`MAX_RECOVERY_DELAY_LEDGERS`].
    InvalidRecoveryDelay = 4,
    /// The contract is not paused, so it cannot be recovered from.
    NotPaused = 5,
}

impl core::fmt::Display for PauseError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::ContractPaused => write!(f, "contract is paused; operation not permitted"),
            Self::RecoveryPending => write!(
                f,
                "pause recovery is time-locked; the contract cannot be resumed yet"
            ),
            Self::RecoveryNotScheduled => write!(f, "no pause recovery is scheduled"),
            Self::InvalidRecoveryDelay => write!(f, "recovery delay is out of range"),
            Self::NotPaused => write!(f, "contract is not paused"),
        }
    }
}

pub fn get_suggestion(error: PauseError) -> Symbol {
    match error {
        PauseError::ContractPaused => symbol_short!("PAUSED"),
        PauseError::RecoveryPending => symbol_short!("LOCKED"),
        PauseError::RecoveryNotScheduled => symbol_short!("NO_RECOV"),
        PauseError::InvalidRecoveryDelay => symbol_short!("BAD_DELAY"),
        PauseError::NotPaused => symbol_short!("UNPAUSED"),
    }
}

#[derive(Clone)]
#[contracttype]
pub enum PauseDataKey {
    Paused,
    /// Ledger at which a scheduled recovery matures. `0` — the value stored by
    /// `pause` and by `cancel_recovery` — means "no recovery armed". A scheduled
    /// recovery always lands on `now + delay` with `delay >= 1`, so `0` is an
    /// unambiguous sentinel (#711).
    RecoveryEta,
}

#[derive(Clone)]
#[contracttype]
pub struct ContractPausedEvent {
    pub admin: Address,
}

#[derive(Clone)]
#[contracttype]
pub struct ContractUnpausedEvent {
    pub admin: Address,
}

/// Emitted when [`schedule_recovery`] arms the time-lock (#711).
#[derive(Clone)]
#[contracttype]
pub struct RecoveryScheduledEvent {
    pub admin: Address,
    /// Ledger at which `unpause` will be allowed to run.
    pub eta_ledger: u32,
    /// Ledgers the admin asked to wait, for off-chain reporting.
    pub delay_ledgers: u32,
}

/// Emitted when a time-locked recovery completes, i.e. when a scheduled
/// `unpause` is finally allowed through (#711).
#[derive(Clone)]
#[contracttype]
pub struct RecoveryCompletedEvent {
    pub admin: Address,
    /// The matured lock that was cleared by this call.
    pub eta_ledger: u32,
}

// ── Visibility ───────────────────────────────────────────────────────────────

/// `true` when the contract is currently paused.
///
/// Public counterpart to the private helper `shared::health` uses, so monitors
/// and integrators do not have to reach into storage keys (#711).
pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&PauseDataKey::Paused)
        .unwrap_or(false)
}

/// Ledger at which a scheduled recovery matures, or `None` when no recovery is
/// armed. Lets an operator or an off-chain monitor see the time-lock before
/// trying to resume (#711).
pub fn recovery_eta(env: &Env) -> Option<u32> {
    match env.storage().instance().get(&PauseDataKey::RecoveryEta) {
        Some(eta) if eta != 0 => Some(eta),
        _ => None,
    }
}

// ── Guards ───────────────────────────────────────────────────────────────────

/// Typed guard: returns [`PauseError::ContractPaused`] instead of aborting.
///
/// The five consumer contracts should adopt this once their error enums carry a
/// pause variant; that migration is deliberately out of scope for #711.
pub fn require_not_paused_typed(env: &Env) -> Result<(), PauseError> {
    if is_paused(env) {
        Err(PauseError::ContractPaused)
    } else {
        Ok(())
    }
}

/// Guard kept for the existing call sites that use it as a bare statement
/// (`campaign`, `donation`, `withdrawal`, `revenue_sharing`).
///
/// It now aborts with the typed error's description rather than the bare
/// `"contract is paused"` string, so a failed call is attributable to
/// `PauseError::ContractPaused` (#711).
pub fn require_not_paused(env: &Env) {
    if let Err(err) = require_not_paused_typed(env) {
        panic!("{}", err);
    }
}

// ── Pause / recovery ─────────────────────────────────────────────────────────

/// Pause the contract. Any pending recovery is cleared: a fresh stop invalidates
/// whatever the admin had scheduled before (#711).
pub fn pause(env: &Env, admin: &Address) {
    admin.require_auth();
    env.storage().instance().set(&PauseDataKey::Paused, &true);
    env.storage().instance().set(&PauseDataKey::RecoveryEta, &0u32);
    env.events().publish(
        (Symbol::new(env, "contract_paused"),),
        ContractPausedEvent { admin: admin.clone() },
    );
}

/// Arm a time-locked recovery and return the ledger at which it matures.
///
/// Must be called while the contract is paused. `delay_ledgers` is bounded to
/// `1..=MAX_RECOVERY_DELAY_LEDGERS` so that a zero delay cannot be used to
/// dress an instant unpause up as a scheduled one (#711).
pub fn schedule_recovery(
    env: &Env,
    admin: &Address,
    delay_ledgers: u32,
) -> Result<u32, PauseError> {
    admin.require_auth();
    if !is_paused(env) {
        return Err(PauseError::NotPaused);
    }
    if delay_ledgers == 0 || delay_ledgers > MAX_RECOVERY_DELAY_LEDGERS {
        return Err(PauseError::InvalidRecoveryDelay);
    }
    let eta = env
        .ledger()
        .sequence()
        .checked_add(delay_ledgers)
        .ok_or(PauseError::InvalidRecoveryDelay)?;
    env.storage().instance().set(&PauseDataKey::RecoveryEta, &eta);
    env.events().publish(
        (Symbol::new(env, "recovery_scheduled"),),
        RecoveryScheduledEvent {
            admin: admin.clone(),
            eta_ledger: eta,
            delay_ledgers,
        },
    );
    Ok(eta)
}

/// Disarm a pending recovery without resuming the contract. Lets an admin undo
/// a mistaken schedule; the contract stays paused (#711).
pub fn cancel_recovery(env: &Env, admin: &Address) -> Result<(), PauseError> {
    admin.require_auth();
    if recovery_eta(env).is_none() {
        return Err(PauseError::RecoveryNotScheduled);
    }
    env.storage().instance().set(&PauseDataKey::RecoveryEta, &0u32);
    Ok(())
}

/// Typed [`unpause`]: returns [`PauseError::RecoveryPending`] while the
/// time-lock is still running instead of aborting (#711).
pub fn try_unpause(env: &Env, admin: &Address) -> Result<(), PauseError> {
    admin.require_auth();
    // `>=` so recovery becomes possible exactly on the maturing ledger, matching
    // the `created + voting_ledgers` convention used by the rest of the repo.
    if let Some(eta) = recovery_eta(env) {
        if env.ledger().sequence() < eta {
            return Err(PauseError::RecoveryPending);
        }
        env.storage().instance().set(&PauseDataKey::RecoveryEta, &0u32);
        env.events().publish(
            (Symbol::new(env, "recovery_completed"),),
            RecoveryCompletedEvent {
                admin: admin.clone(),
                eta_ledger: eta,
            },
        );
    }
    env.storage().instance().set(&PauseDataKey::Paused, &false);
    env.events().publish(
        (Symbol::new(env, "contract_unpaused"),),
        ContractUnpausedEvent { admin: admin.clone() },
    );
    Ok(())
}

/// Resume the contract.
///
/// Signature frozen: the five consumer contracts call `unpause(&env, &admin)`
/// as a bare statement, so this cannot return a `Result` without breaking them
/// and failing `clippy -D warnings`. It refuses while a time-locked recovery is
/// still running by aborting with `PauseError::RecoveryPending`'s description;
/// new call sites should prefer [`try_unpause`] (#711).
pub fn unpause(env: &Env, admin: &Address) {
    if let Err(err) = try_unpause(env, admin) {
        panic!("{}", err);
    }
}
