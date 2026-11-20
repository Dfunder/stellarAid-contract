//! Re-entrancy guard, shared by every StellarAid contract (issue #762).
//!
//! # Model
//!
//! A single instance-storage flag — [`LockKey::Locked`] — is set on entry to a
//! guarded entry point and cleared on exit. A call that arrives while the flag
//! is set is rejected with [`ReentrancyError::Reentrant`] *before* it reads or
//! writes any state, so a malicious callee cannot re-enter a half-applied
//! mutation.
//!
//! # Why a storage flag and not an in-memory variable
//!
//! The host gives each contract invocation its own frame, so an in-memory flag
//! would be invisible to a re-entrant call. Instance storage is part of the
//! invocation's transactional state, which is exactly the scope the guard needs.
//!
//! # Clearing on the error path
//!
//! [`with_reentrancy_guard`] releases the lock after `f` returns, covering
//! early returns and `Err` results. If `f` panics or aborts, the entire
//! invocation reverts and the instance storage — the lock included — is
//! discarded with it, so a failed call cannot wedge the contract. This is the
//! same property `contracts/escrow` relies on for its own
//! `storage::with_reentrancy_guard`.
//!
//! # Do not nest
//!
//! The guard is a flat, non-reentrant flag: it does not count depth. Wrapping
//! two entry points where one calls the other (for example `approve_campaign`
//! delegating to `update_campaign_status`) would make the inner call trip the
//! guard and fail. Guard the **outermost** entry point of a call tree only.
//!
//! # Placement
//!
//! A guarded function must follow checks-effects-interactions with the guard
//! acquired **first**, before any storage read that a re-entrant call could
//! observe as half-applied. The flag lives in instance storage, so it is
//! covered by the same rollback semantics as the rest of the invocation.

use soroban_sdk::{contracterror, contracttype, panic_with_error, symbol_short, Env, Symbol};

/// Instance-storage keys owned by the guard.
///
/// Named `Locked` so the flag is `DataKey::Locked` in spirit at every call
/// site: one key, one meaning, one shared key space.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LockKey {
    /// Set while a guarded entry point is executing.
    Locked = 0,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReentrancyError {
    /// A re-entrant call was rejected; the lock was already held. Code `9` is
    /// shared with `EscrowError::Reentrant` so an indexer sees one re-entrancy
    /// code across the whole system.
    Reentrant = 9,
}

impl core::fmt::Display for ReentrancyError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::Reentrant => write!(f, "re-entrant call rejected"),
        }
    }
}

pub fn get_suggestion(error: ReentrancyError) -> Symbol {
    match error {
        ReentrancyError::Reentrant => symbol_short!("REENTRY"),
    }
}

/// Whether the re-entrancy lock is currently held.
#[inline]
pub fn is_locked(env: &Env) -> bool {
    env.storage().instance().has(&LockKey::Locked)
}

/// Acquire the lock, rejecting a re-entrant call.
///
/// Panics with [`ReentrancyError::Reentrant`] if the lock is already held.
#[inline]
pub fn acquire(env: &Env) {
    if is_locked(env) {
        panic_with_error!(env, ReentrancyError::Reentrant);
    }
    env.storage().instance().set(&LockKey::Locked, &true);
}

/// Release the lock.
#[inline]
pub fn release(env: &Env) {
    env.storage().instance().remove(&LockKey::Locked);
}

/// Runs `f` under the re-entrancy guard.
///
/// Rejects any call that arrives while a guarded entry point is still
/// executing, and always releases the lock once `f` returns — including on the
/// error path. If `f` panics, the invocation reverts and the lock is discarded
/// with the rest of the instance storage.
///
/// ```ignore
/// let total = shared::guard::with_reentrancy_guard(&env, || 1_i128 + 1);
/// assert_eq!(total, 2);
/// assert!(!shared::guard::is_locked(&env));
/// ```
pub fn with_reentrancy_guard<F, T>(env: &Env, f: F) -> T
where
    F: FnOnce() -> T,
{
    acquire(env);
    let result = f();
    release(env);
    result
}

// ── Input guards (issue #945) ────────────────────────────────────────────────

/// Pure input-guard failures shared by the contracts that used to define their
/// own `require!` macro.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputGuardError {
    /// A positive amount was required but zero was supplied.
    ZeroAmount = 20,
}

/// Pure half of an input guard: no host access, so it can be unit-tested and
/// reused from off-chain tooling.
#[inline]
pub fn check_nonzero(amount: i128) -> Result<(), InputGuardError> {
    if amount == 0 {
        Err(InputGuardError::ZeroAmount)
    } else {
        Ok(())
    }
}

/// Turn a failed pure guard into a typed contract error at the call site.
///
/// This is the single `require!` used by the workspace; per-contract copies
/// should be reduced to thin wrappers over the shared `check_*` functions.
#[macro_export]
macro_rules! require {
    ($env:expr, $result:expr) => {
        if let Err(err) = $result {
            soroban_sdk::panic_with_error!($env, err);
        }
    };
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    #[test]
    fn lock_is_free_before_and_after_the_guarded_call() {
        let env = Env::default();
        assert!(!is_locked(&env));
        let out = with_reentrancy_guard(&env, || {
            assert!(is_locked(&env));
            7_u32
        });
        assert_eq!(out, 7);
        assert!(!is_locked(&env));
    }

    #[test]
    fn nested_guard_is_rejected() {
        let env = Env::default();
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_reentrancy_guard(&env, || {
                // Simulates a re-entrant call arriving mid-mutation.
                acquire(&env);
            });
        }));
        assert!(res.is_err());
    }

    #[test]
    fn error_path_still_releases_the_lock() {
        let env = Env::default();
        let out: Result<u32, u32> = with_reentrancy_guard(&env, || Err(1));
        assert_eq!(out, Err(1));
        assert!(!is_locked(&env));
    }

    #[test]
    fn error_code_is_stable_at_nine() {
        // Pinned: an indexer decodes re-entrancy across contracts by this value.
        assert_eq!(ReentrancyError::Reentrant as u32, 9);
        assert_eq!(LockKey::Locked as u32, 0);
    }

    #[test]
    fn shared_input_guard_rejects_zero() {
        assert_eq!(check_nonzero(0), Err(InputGuardError::ZeroAmount));
        assert_eq!(check_nonzero(1), Ok(()));
    }

    // Health threshold coverage (issue #930). The SLA/anomaly thresholds are
    // the guardrails that decide whether rollout triggers, so they are checked
    // here alongside the other safety guards.
    #[test]
    fn health_thresholds_are_inclusive_at_the_boundary() {
        use crate::health::{self, AlertConfig, HealthMetrics, HealthStatus};

        let env = Env::default();
        let config = AlertConfig {
            degraded_error_bps: 100,
            unhealthy_error_bps: 500,
            stall_ledgers: 60,
            alert_cooldown_ledgers: 60,
            alerting_enabled: false,
        };
        let metrics = |errors: u64, total: u64| HealthMetrics {
            ok_count: total - errors,
            error_count: errors,
            last_ok_ledger: 1,
            last_error_ledger: 1,
            paused: false,
        };

        // 1/1000 = 10 bps: healthy.
        assert_eq!(health::error_bps(&metrics(1, 1000)), 10);
        assert_eq!(
            health::classify(&env, &metrics(1, 1000), &config),
            (HealthStatus::Healthy, false)
        );
        // 1/100 = 100 bps: exactly the degraded threshold.
        assert_eq!(
            health::classify(&env, &metrics(1, 100), &config).0,
            HealthStatus::Degraded
        );
        // 1/20 = 500 bps: exactly the unhealthy threshold.
        assert_eq!(
            health::classify(&env, &metrics(1, 20), &config).0,
            HealthStatus::Unhealthy
        );
        // A single sample with no errors must not be flagged.
        assert_eq!(
            health::classify(&env, &metrics(0, 1), &config).0,
            HealthStatus::Healthy
        );
    }
}
