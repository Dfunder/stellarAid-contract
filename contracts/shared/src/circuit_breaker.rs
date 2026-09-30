//! Circuit breaker for emergency halting and automatic anomaly response (closes #771).
//!
//! A circuit breaker wraps the existing [`crate::pause`] / [`crate::health`]
//! infrastructure to provide three capabilities:
//!
//! 1. **Monitoring** — every contract entry point that calls
//!    [`record_success`] or [`record_failure`] feeds the health module's error
//!    counters. The circuit breaker reads those counters to decide whether an
//!    automatic halt is needed.
//!
//! 2. **Automatic halt** — when the error rate crosses a configurable threshold
//!    (default 5 %, see [`DEFAULT_HALT_ERROR_BPS`]), [`maybe_auto_halt`] pauses
//!    the contract automatically and emits a `cb_autohalt` event. The pause
//!    reuses the time-locked recovery flow from [`crate::pause`], so an
//!    operator must wait out the lock before resuming.
//!
//! 3. **Manual control** — [`halt`] / [`resume`] let an admin stop and start
//!    the contract on demand. These delegate to [`crate::pause`] /
//!    [`crate::pause::try_unpause`] so there is a single pause flag and a
//!    single recovery window.
//!
//! 4. **Recovery** — [`crate::pause::schedule_recovery`] +
//!    [`crate::pause::try_unpause`] provide the time-locked recovery flow.
//!    The circuit breaker exposes [`recover`] as a typed wrapper that waits for
//!    the lock to mature, then resumes.
//!
//! ## Usage
//!
//! At the top of every mutating entry point:
//!
//! ```ignore
//! shared::circuit_breaker::require_not_halted(&env)?;
//! ```
//!
//! At the end of every entry point (success or failure):
//!
//! ```ignore
//! shared::circuit_breaker::record_success(&env);
//! // or, on an error path:
//! shared::circuit_breaker::record_failure(&env);
//! ```

use crate::health;
use crate::pause;
use crate::HealthMetrics;
use soroban_sdk::{contracterror, contracttype, symbol_short, Address, Env, Symbol};

/// Error rate (basis points) at or above which the circuit breaker auto-halts.
/// 500 = 5 %.
pub const DEFAULT_HALT_ERROR_BPS: u32 = 500;

/// Minimum total calls before the circuit breaker can evaluate the error rate.
/// Below this volume the rate is too noisy to act on.
pub const MIN_SAMPLE_FOR_HALT: u64 = 10;

/// Minimum ledgers between two automatic halts, to avoid flapping.
pub const MIN_HALT_INTERVAL_LEDGERS: u32 = 60;

/// Typed failures of the circuit breaker.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CircuitBreakerError {
    /// The circuit breaker is tripped; operations are halted.
    Halted = 1,
    /// An automatic halt is already in progress.
    AlreadyHalted = 2,
    /// The contract is not halted, so it cannot be resumed.
    NotHalted = 3,
    /// A scheduled recovery has not matured yet.
    RecoveryPending = 4,
    /// The halt threshold is out of range (0 or > 10 000 bps).
    InvalidThreshold = 5,
    /// Too few samples to evaluate the error rate.
    InsufficientSamples = 6,
}

impl core::fmt::Display for CircuitBreakerError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::Halted => write!(f, "circuit breaker is tripped; operations halted"),
            Self::AlreadyHalted => write!(f, "circuit breaker is already halted"),
            Self::NotHalted => write!(f, "circuit breaker is not halted"),
            Self::RecoveryPending => write!(f, "recovery is time-locked and has not matured"),
            Self::InvalidThreshold => write!(f, "halt threshold must be between 1 and 10000 bps"),
            Self::InsufficientSamples => write!(f, "too few sample calls to evaluate error rate"),
        }
    }
}

pub fn get_suggestion(error: CircuitBreakerError) -> Symbol {
    match error {
        CircuitBreakerError::Halted => symbol_short!("HALTED"),
        CircuitBreakerError::AlreadyHalted => symbol_short!("DUP_HALT"),
        CircuitBreakerError::NotHalted => symbol_short!("RUNNING"),
        CircuitBreakerError::RecoveryPending => symbol_short!("LOCKED"),
        CircuitBreakerError::InvalidThreshold => symbol_short!("BAD_THR"),
        CircuitBreakerError::InsufficientSamples => symbol_short!("FEW_SAMP"),
    }
}

/// Instance-storage keys owned by the circuit breaker.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CircuitBreakerKey {
    /// Admin address that can manually halt / resume.
    Admin,
    /// Error rate in bps at or above which an automatic halt fires.
    HaltErrorBps,
    /// Ledger at which the last automatic or manual halt occurred, for flapping
    /// prevention.
    LastHaltLedger,
}

/// Configuration for the circuit breaker's automatic halt behavior.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CircuitBreakerConfig {
    /// The admin address (also used by [`crate::pause`]).
    pub admin: Address,
    /// Error rate threshold in bps (1..=10000).
    pub halt_error_bps: u32,
}

/// Emitted when the circuit breaker auto-halts.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AutoHaltedEvent {
    pub admin: Address,
    pub error_bps: u32,
    pub threshold_bps: u32,
    pub ledger: u32,
}

/// Emitted when the circuit breaker is manually halted.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManualHaltedEvent {
    pub admin: Address,
    pub ledger: u32,
}

/// Emitted when the circuit breaker resumes after a halt + recovery.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CircuitResumedEvent {
    pub admin: Address,
    pub ledger: u32,
}

// ── Configuration ─────────────────────────────────────────────────────────────

/// Set or update the circuit-breaker configuration (admin only).
///
/// This does **not** pause the contract; it only configures the thresholds
/// used by [`maybe_auto_halt`]. The admin address is stored so that manual
/// halt/resume can be authorized without a separate admin lookup.
pub fn set_config(env: &Env, config: &CircuitBreakerConfig) {
    config.admin.require_auth();
    if config.halt_error_bps == 0 || config.halt_error_bps > 10_000 {
        soroban_sdk::panic_with_error!(env, CircuitBreakerError::InvalidThreshold);
    }
    env.storage()
        .instance()
        .set(&CircuitBreakerKey::Admin, &config.admin);
    env.storage()
        .instance()
        .set(&CircuitBreakerKey::HaltErrorBps, &config.halt_error_bps);
    env.events()
        .publish((symbol_short!("cb_cfg"),), (config.admin.clone(), config.halt_error_bps));
}

/// Read the configured halt threshold, falling back to the default.
pub fn get_halt_error_bps(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&CircuitBreakerKey::HaltErrorBps)
        .unwrap_or(DEFAULT_HALT_ERROR_BPS)
}

/// Read the configured admin, if any.
pub fn get_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&CircuitBreakerKey::Admin)
}

/// Read the ledger of the last halt, for flapping prevention.
pub fn last_halt_ledger(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&CircuitBreakerKey::LastHaltLedger)
        .unwrap_or(0)
}

fn set_last_halt_ledger(env: &Env, ledger: u32) {
    env.storage()
        .instance()
        .set(&CircuitBreakerKey::LastHaltLedger, &ledger);
}

// ── Monitoring ───────────────────────────────────────────────────────────────

/// Record a successful call. Updates the health module's OK counter.
pub fn record_success(env: &Env) {
    health::record_ok(env);
}

/// Record a failed call. Updates the health module's error counter and
/// then evaluates whether an automatic halt should fire.
pub fn record_failure(env: &Env) -> bool {
    health::record_error(env);
    maybe_auto_halt(env)
}

/// Returns `true` if the circuit breaker is currently halted (contract paused).
/// Delegates to [`crate::pause::is_paused`] so there is a single pause flag.
pub fn is_halted(env: &Env) -> bool {
    pause::is_paused(env)
}

/// Guard that rejects a call while the circuit is halted.
///
/// Returns `Err(CircuitBreakerError::Halted)` when paused, `Ok(())` otherwise.
pub fn require_not_halted(env: &Env) -> Result<(), CircuitBreakerError> {
    if is_halted(env) {
        Err(CircuitBreakerError::Halted)
    } else {
        Ok(())
    }
}

// ── Automatic halt ─────────────────────────────────────────────────────────────

/// Evaluate the current error rate and, if it crosses the threshold,
/// automatically halt the contract.
///
/// The auto-halt reuses [`crate::pause::pause`] so the existing time-locked
/// recovery flow applies — an operator must `schedule_recovery` + wait the lock
/// before resuming.
///
/// Returns `true` if an automatic halt was triggered.
pub fn maybe_auto_halt(env: &Env) -> bool {
    if is_halted(env) {
        return false;
    }

    let metrics: HealthMetrics = health::get_metrics(env);
    let total = metrics.ok_count.saturating_add(metrics.error_count);

    if total < MIN_SAMPLE_FOR_HALT {
        return false;
    }

    let error_bps = health::error_bps(&metrics);
    let threshold = get_halt_error_bps(env);

    if error_bps < threshold {
        return false;
    }

    // Flapping guard: don't auto-halt again within `MIN_HALT_INTERVAL_LEDGERS`.
    let now = env.ledger().sequence();
    if now.saturating_sub(last_halt_ledger(env)) < MIN_HALT_INTERVAL_LEDGERS {
        return false;
    }

    let admin = match get_admin(env) {
        Some(a) => a,
        None => return false,
    };

    pause::pause(env, &admin);
    set_last_halt_ledger(env, now);

    env.events().publish(
        (symbol_short!("cb_ahlt"),),
        AutoHaltedEvent {
            admin: admin.clone(),
            error_bps,
            threshold_bps: threshold,
            ledger: now,
        },
    );

    true
}

// ── Manual control ─────────────────────────────────────────────────────────────

/// Manually halt the contract (admin only).
///
/// Delegates to [`crate::pause::pause`]. Any previously scheduled recovery
/// is cleared.
pub fn halt(env: &Env, admin: &Address) -> Result<(), CircuitBreakerError> {
    admin.require_auth();
    if is_halted(env) {
        return Err(CircuitBreakerError::AlreadyHalted);
    }
    pause::pause(env, admin);
    set_last_halt_ledger(env, env.ledger().sequence());
    env.events().publish(
        (symbol_short!("cb_halt"),),
        ManualHaltedEvent {
            admin: admin.clone(),
            ledger: env.ledger().sequence(),
        },
    );
    Ok(())
}

/// Manually resume the contract (admin only).
///
/// Delegates to [`crate::pause::try_unpause`]. Respects any time-locked
/// recovery that was scheduled via [`crate::pause::schedule_recovery`].
pub fn resume(env: &Env, admin: &Address) -> Result<(), CircuitBreakerError> {
    admin.require_auth();
    if !is_halted(env) {
        return Err(CircuitBreakerError::NotHalted);
    }
    pause::try_unpause(env, admin).map_err(|e| match e {
        pause::PauseError::RecoveryPending => CircuitBreakerError::RecoveryPending,
        _ => CircuitBreakerError::NotHalted,
    })?;
    env.events().publish(
        (symbol_short!("cb_rsm"),),
        CircuitResumedEvent {
            admin: admin.clone(),
            ledger: env.ledger().sequence(),
        },
    );
    Ok(())
}

/// Query the current circuit-breaker state and thresholds.
pub fn get_state(env: &Env) -> CircuitBreakerState {
    let metrics = health::get_metrics(env);
    let total = metrics.ok_count.saturating_add(metrics.error_count);
    let bps = if total == 0 { 0 } else { health::error_bps(&metrics) };
    CircuitBreakerState {
        halted: is_halted(env),
        error_bps: bps,
        total_calls: total,
        ok_count: metrics.ok_count,
        error_count: metrics.error_count,
        halt_error_bps: get_halt_error_bps(env),
        last_halt_ledger: last_halt_ledger(env),
        recovery_eta: pause::recovery_eta(env),
    }
}

/// Public snapshot of the circuit breaker's runtime state.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CircuitBreakerState {
    /// `true` when the contract is currently halted.
    pub halted: bool,
    /// Current error rate in basis points.
    pub error_bps: u32,
    /// Total number of recorded calls (ok + error).
    pub total_calls: u64,
    /// Count of successful calls.
    pub ok_count: u64,
    /// Count of failed calls.
    pub error_count: u64,
    /// Threshold at or above which an automatic halt fires.
    pub halt_error_bps: u32,
    /// Ledger of the last halt (for flapping prevention).
    pub last_halt_ledger: u32,
    /// Some(ledger) when a time-locked recovery is scheduled.
    pub recovery_eta: Option<u32>,
}

// ── Recovery procedures ────────────────────────────────────────────────────────

/// Schedule a time-locked recovery (admin only).
///
/// Reuses [`crate::pause::schedule_recovery`]. The operator calls this while
/// the contract is halted, waits for the lock to mature, then calls
/// [`resume`].
pub fn schedule_recovery(
    env: &Env,
    admin: &Address,
    delay_ledgers: u32,
) -> Result<u32, CircuitBreakerError> {
    admin.require_auth();
    if !is_halted(env) {
        return Err(CircuitBreakerError::NotHalted);
    }
    pause::schedule_recovery(env, admin, delay_ledgers)
        .map_err(|e| match e {
            pause::PauseError::NotPaused => CircuitBreakerError::NotHalted,
            pause::PauseError::InvalidRecoveryDelay => CircuitBreakerError::InvalidThreshold,
            _ => CircuitBreakerError::RecoveryPending,
        })
}

/// Cancel a previously scheduled recovery (admin only). The contract stays halted.
pub fn cancel_recovery(env: &Env, admin: &Address) -> Result<(), CircuitBreakerError> {
    admin.require_auth();
    pause::cancel_recovery(env, admin).map_err(|e| match e {
        pause::PauseError::RecoveryNotScheduled => CircuitBreakerError::NotHalted,
        _ => CircuitBreakerError::RecoveryPending,
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health;
    use crate::pause;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::testutils::Ledger as _;

    fn setup() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env
    }

    #[test]
    fn default_halt_threshold_is_five_percent() {
        assert_eq!(DEFAULT_HALT_ERROR_BPS, 500);
    }

    #[test]
    fn valid_threshold_constants() {
        assert!(DEFAULT_HALT_ERROR_BPS <= 10_000);
        assert!(MIN_SAMPLE_FOR_HALT >= 1);
        assert!(MIN_HALT_INTERVAL_LEDGERS >= 1);
    }

    #[test]
    fn error_bps_calculation() {
        // 1 error out of 10 total = 1000 bps
        let metrics = HealthMetrics {
            ok_count: 9,
            error_count: 1,
            last_ok_ledger: 10,
            last_error_ledger: 10,
            paused: false,
        };
        assert_eq!(health::error_bps(&metrics), 1000);

        // 5 errors out of 100 = 500 bps
        let metrics = HealthMetrics {
            ok_count: 95,
            error_count: 5,
            last_ok_ledger: 100,
            last_error_ledger: 100,
            paused: false,
        };
        assert_eq!(health::error_bps(&metrics), 500);
    }

    #[test]
    fn is_halted_reflects_pause_flag() {
        let env = setup();
        assert!(!is_halted(&env));

        let admin = Address::generate(&env);
        pause::pause(&env, &admin);
        assert!(is_halted(&env));

        pause::try_unpause(&env, &admin).unwrap();
        assert!(!is_halted(&env));
    }

    #[test]
    fn require_not_halted_allows_when_running() {
        let env = setup();
        assert!(require_not_halted(&env).is_ok());
    }

    #[test]
    fn require_not_halted_rejects_when_halted() {
        let env = setup();
        let admin = Address::generate(&env);
        pause::pause(&env, &admin);
        let err = require_not_halted(&env).unwrap_err();
        assert_eq!(err, CircuitBreakerError::Halted);
    }

    #[test]
    fn halt_and_resume_lifecycle() {
        let env = setup();
        let admin = Address::generate(&env);

        set_config(&env, &CircuitBreakerConfig {
            admin: admin.clone(),
            halt_error_bps: DEFAULT_HALT_ERROR_BPS,
        });

        assert!(!is_halted(&env));
        halt(&env, &admin).unwrap();
        assert!(is_halted(&env));

        // Halting again should fail
        let err = halt(&env, &admin).unwrap_err();
        assert_eq!(err, CircuitBreakerError::AlreadyHalted);

        // Resume
        resume(&env, &admin).unwrap();
        assert!(!is_halted(&env));

        // Resuming again should fail
        let err = resume(&env, &admin).unwrap_err();
        assert_eq!(err, CircuitBreakerError::NotHalted);
    }

    #[test]
    fn auto_halt_triggers_when_threshold_exceeded() {
        let env = setup();
        let admin = Address::generate(&env);

        set_config(&env, &CircuitBreakerConfig {
            admin: admin.clone(),
            halt_error_bps: 500, // 5%
        });

        // Record 5 errors out of 10 calls (50% error rate, well above 5%)
        for _ in 0..5 {
            record_failure(&env);
        }
        for _ in 0..5 {
            record_success(&env);
        }

        assert!(is_halted(&env), "should auto-halt when error rate exceeds threshold");
        assert_eq!(last_halt_ledger(&env), env.ledger().sequence());
    }

    #[test]
    fn auto_halt_does_not_trigger_below_threshold() {
        let env = setup();
        let admin = Address::generate(&env);

        set_config(&env, &CircuitBreakerConfig {
            admin: admin.clone(),
            halt_error_bps: 500, // 5%
        });

        for _ in 0..95 {
            record_success(&env);
        }
        for _ in 0..5 {
            record_failure(&env);
        }
        // 5% exactly — not above threshold, should not halt
        assert!(!is_halted(&env));
    }

    #[test]
    fn auto_halt_does_not_trigger_with_few_samples() {
        let env = setup();
        let admin = Address::generate(&env);

        set_config(&env, &CircuitBreakerConfig {
            admin: admin.clone(),
            halt_error_bps: 100, // 1% threshold
        });

        // Only 2 samples — below MIN_SAMPLE_FOR_HALT
        record_failure(&env);
        record_failure(&env);
        assert!(!is_halted(&env));
    }

    #[test]
    fn get_state_reports_correct_values() {
        let env = setup();
        let admin = Address::generate(&env);

        set_config(&env, &CircuitBreakerConfig {
            admin: admin.clone(),
            halt_error_bps: 500,
        });

        record_success(&env);
        record_success(&env);
        record_failure(&env);

        let state = get_state(&env);
        assert!(!state.halted);
        assert_eq!(state.total_calls, 3);
        assert_eq!(state.ok_count, 2);
        assert_eq!(state.error_count, 1);
        assert_eq!(state.halt_error_bps, 500);
    }

    #[test]
    fn time_locked_recovery_workflow() {
        let env = setup();
        let admin = Address::generate(&env);

        set_config(&env, &CircuitBreakerConfig {
            admin: admin.clone(),
            halt_error_bps: 500,
        });

        halt(&env, &admin).unwrap();
        assert!(is_halted(&env));

        // Schedule a 10-ledger recovery
        let eta = schedule_recovery(&env, &admin, 10).unwrap();
        assert_eq!(eta, env.ledger().sequence() + 10);

        // Resume immediately should fail (recovery pending)
        let err = resume(&env, &admin).unwrap_err();
        assert_eq!(err, CircuitBreakerError::RecoveryPending);

        // Advance past the lock
        env.ledger().with_mut(|l| l.sequence_number += 11);

        // Now resume should work
        resume(&env, &admin).unwrap();
        assert!(!is_halted(&env));
    }

    #[test]
    fn circuit_breaker_error_codes_are_unique() {
        let codes = [
            CircuitBreakerError::Halted as u32,
            CircuitBreakerError::AlreadyHalted as u32,
            CircuitBreakerError::NotHalted as u32,
            CircuitBreakerError::RecoveryPending as u32,
            CircuitBreakerError::InvalidThreshold as u32,
            CircuitBreakerError::InsufficientSamples as u32,
        ];
        for i in 0..codes.len() {
            for j in (i + 1)..codes.len() {
                assert_ne!(codes[i], codes[j], "duplicate error code");
            }
        }
    }
}
