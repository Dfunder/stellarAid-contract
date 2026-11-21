//! Contract health monitoring (closes #678).
//!
//! Every Lumora contract exposes the same on-chain health surface:
//!
//! 1. **Metrics** — success/error counters, last-activity ledgers, pause flag.
//! 2. **Health check** — a read entry point that classifies the contract as
//!    `Healthy`, `Degraded`, or `Unhealthy` against configured SLA thresholds.
//! 3. **Anomaly detection** — error-rate spikes and activity stalls.
//! 4. **Alerting** — optional `hlth_alrt` events, rate-limited by cooldown.
//! 5. **SLA targets** — documented, queryable constants used by monitors.
//!
//! Off-chain monitors invoke `health_check` on a schedule (see `docs/SLA.md`).
//! Operators (or the contract itself) call `record_ok` / `record_error` so the
//! counters reflect real invocation outcomes.

use crate::pause::PauseDataKey;
use crate::rollout::{self, RolloutState};
use crate::storage::instance;
use soroban_sdk::{contracttype, symbol_short, Env};
use soroban_sdk::{contracterror, contracttype, symbol_short, Env};

/// Typed failures of the health module, so callers branch on a discriminant
/// instead of matching panic prose.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HealthError {
    /// Alert thresholds are inverted, exceed 10000 bps, or use a zero window.
    InvalidAlertConfig = 1,
}

/// Target availability in basis points: 9990 = 99.90%.
pub const SLA_AVAILABILITY_BPS: u32 = 9_990;
/// Error-rate SLO in basis points: 10 = 0.10% (complements 99.90% availability).
pub const SLA_MAX_ERROR_BPS: u32 = 10;
/// Default error-rate that marks the contract Degraded (1%).
pub const DEFAULT_DEGRADED_ERROR_BPS: u32 = 100;
/// Default error-rate that marks the contract Unhealthy (5%).
pub const DEFAULT_UNHEALTHY_ERROR_BPS: u32 = 500;
/// Default inactivity window before a stall is flagged (~1 day at 5s/ledger).
pub const DEFAULT_STALL_LEDGERS: u32 = 17_280;
/// Minimum ledgers between consecutive alert events.
pub const DEFAULT_ALERT_COOLDOWN_LEDGERS: u32 = 60;
/// Off-chain monitors should poll at least this often (~5 min at 5s/ledger).
pub const SLA_HEALTH_CHECK_MAX_LEDGERS: u32 = 60;

const BPS_DENOM: u32 = 10_000;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HealthKey {
    Metrics,
    AlertConfig,
    LastAlertLedger,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HealthStatus {
    Healthy = 0,
    Degraded = 1,
    Unhealthy = 2,
}

/// Counters and freshness data used by health checks and anomaly detection.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HealthMetrics {
    pub ok_count: u64,
    pub error_count: u64,
    pub last_ok_ledger: u32,
    pub last_error_ledger: u32,
    pub paused: bool,
}

/// Thresholds that map raw metrics onto a [`HealthStatus`] and control alerts.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AlertConfig {
    /// Error rate (bps) at or above which status becomes `Degraded`.
    pub degraded_error_bps: u32,
    /// Error rate (bps) at or above which status becomes `Unhealthy`.
    pub unhealthy_error_bps: u32,
    /// Ledgers without activity after which a stall anomaly is raised.
    pub stall_ledgers: u32,
    /// Ledgers to wait between successive `hlth_alrt` events.
    pub alert_cooldown_ledgers: u32,
    /// When `false`, anomalies are still detected but no events are emitted.
    pub alerting_enabled: bool,
}

/// Published SLA numbers so monitors do not have to hard-code them.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlaTargets {
    pub availability_bps: u32,
    pub max_error_bps: u32,
    pub degraded_error_bps: u32,
    pub unhealthy_error_bps: u32,
    pub stall_ledgers: u32,
    pub health_check_max_ledgers: u32,
}

/// Result of [`health_check`]: status, raw metrics, derived error rate, anomaly flag.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HealthReport {
    pub status: HealthStatus,
    pub metrics: HealthMetrics,
    pub error_bps: u32,
    pub anomaly: bool,
    pub stalled: bool,
}

pub fn default_alert_config() -> AlertConfig {
    AlertConfig {
        degraded_error_bps: DEFAULT_DEGRADED_ERROR_BPS,
        unhealthy_error_bps: DEFAULT_UNHEALTHY_ERROR_BPS,
        stall_ledgers: DEFAULT_STALL_LEDGERS,
        alert_cooldown_ledgers: DEFAULT_ALERT_COOLDOWN_LEDGERS,
        alerting_enabled: true,
    }
}

pub fn sla_targets() -> SlaTargets {
    SlaTargets {
        availability_bps: SLA_AVAILABILITY_BPS,
        max_error_bps: SLA_MAX_ERROR_BPS,
        degraded_error_bps: DEFAULT_DEGRADED_ERROR_BPS,
        unhealthy_error_bps: DEFAULT_UNHEALTHY_ERROR_BPS,
        stall_ledgers: DEFAULT_STALL_LEDGERS,
        health_check_max_ledgers: SLA_HEALTH_CHECK_MAX_LEDGERS,
    }
}

pub fn get_alert_config(env: &Env) -> AlertConfig {
    instance::get_or_else(env, &HealthKey::AlertConfig, default_alert_config)
}

/// Persist alerting thresholds. Panics if the bps values are inverted or > 10000.
pub fn set_alert_config(env: &Env, config: AlertConfig) {
    if config.degraded_error_bps > BPS_DENOM
        || config.unhealthy_error_bps > BPS_DENOM
        || config.degraded_error_bps > config.unhealthy_error_bps
        || config.stall_ledgers == 0
        || config.alert_cooldown_ledgers == 0
    {
        soroban_sdk::panic_with_error!(env, HealthError::InvalidAlertConfig);
    }
    instance::set(env, &HealthKey::AlertConfig, &config);
    env.events()
        .publish((symbol_short!("alrt_cfg"),), config.unhealthy_error_bps);
}

fn is_paused(env: &Env) -> bool {
    instance::get_or(env, &PauseDataKey::Paused, false)
}

pub fn get_metrics(env: &Env) -> HealthMetrics {
    let mut metrics: HealthMetrics = instance::get_or(
        env,
        &HealthKey::Metrics,
        HealthMetrics {
            ok_count: 0,
            error_count: 0,
            last_ok_ledger: 0,
            last_error_ledger: 0,
            paused: false,
        },
    );
    metrics.paused = is_paused(env);
    metrics
}

fn save_metrics(env: &Env, metrics: &HealthMetrics) {
    instance::set(env, &HealthKey::Metrics, metrics);
}

/// Record one invocation outcome: `ok = true` is a success sample,
/// `ok = false` an error sample. Backs the `report_outcome` entry point.
pub fn record_outcome(env: &Env, ok: bool) {
    if ok {
        record_ok(env)
    } else {
        record_error(env)
    }
}

/// Record a successful invocation sample.
pub fn record_ok(env: &Env) {
    let mut metrics = get_metrics(env);
    metrics.ok_count = metrics.ok_count.saturating_add(1);
    metrics.last_ok_ledger = env.ledger().sequence();
    save_metrics(env, &metrics);
}

/// Record a failed invocation sample.
pub fn record_error(env: &Env) {
    let mut metrics = get_metrics(env);
    metrics.error_count = metrics.error_count.saturating_add(1);
    metrics.last_error_ledger = env.ledger().sequence();
    save_metrics(env, &metrics);
}

#[allow(unknown_lints, clippy::manual_checked_ops)]
pub fn error_bps(metrics: &HealthMetrics) -> u32 {
    let total = (metrics.ok_count as u128).saturating_add(metrics.error_count as u128);
    if total == 0 {
        0
    } else {
        ((metrics.error_count as u128) * (BPS_DENOM as u128) / total) as u32
    }
}

fn last_activity_ledger(metrics: &HealthMetrics) -> u32 {
    if metrics.last_ok_ledger > metrics.last_error_ledger {
        metrics.last_ok_ledger
    } else {
        metrics.last_error_ledger
    }
}

pub fn is_stalled(env: &Env, metrics: &HealthMetrics, config: &AlertConfig) -> bool {
    let last = last_activity_ledger(metrics);
    if last == 0 {
        return false;
    }
    env.ledger().sequence().saturating_sub(last) >= config.stall_ledgers
}

pub fn classify(env: &Env, metrics: &HealthMetrics, config: &AlertConfig) -> (HealthStatus, bool) {
    let rate = error_bps(metrics);
    let stalled = is_stalled(env, metrics, config);
    if metrics.paused || rate >= config.unhealthy_error_bps {
        (HealthStatus::Unhealthy, true)
    } else if rate >= config.degraded_error_bps || stalled {
        (HealthStatus::Degraded, true)
    } else {
        (HealthStatus::Healthy, false)
    }
}

/// `true` when the contract is degraded, unhealthy, or stalled.
pub fn detect_anomaly(env: &Env) -> bool {
    let metrics = get_metrics(env);
    let config = get_alert_config(env);
    classify(env, &metrics, &config).1
}

fn maybe_emit_alert(env: &Env, report: &HealthReport, config: &AlertConfig) {
    if !config.alerting_enabled || !report.anomaly {
        return;
    }
    let last: u32 = instance::get_or(env, &HealthKey::LastAlertLedger, 0);
    let now = env.ledger().sequence();
    if last != 0 && now.saturating_sub(last) < config.alert_cooldown_ledgers {
        return;
    }
    instance::set(env, &HealthKey::LastAlertLedger, &now);
    env.events().publish(
        (symbol_short!("hlth_alrt"),),
        (report.status, report.error_bps, report.stalled),
    );
}

/// Classify current health, emit an alert if configured, and return the report.
pub fn health_check(env: &Env) -> HealthReport {
    let metrics = get_metrics(env);
    let config = get_alert_config(env);
    let (status, anomaly) = classify(env, &metrics, &config);
    let report = HealthReport {
        status,
        error_bps: error_bps(&metrics),
        stalled: is_stalled(env, &metrics, &config),
        anomaly,
        metrics,
    };
    maybe_emit_alert(env, &report, &config);
    report
}

/// Everything an operator needs to judge contract health, read in one call.
///
/// Before this existed, answering "is this contract healthy?" took five
/// separate simulations (`detect_anomaly`, `get_health_metrics`,
/// `get_sla_targets`, `get_alert_config`, `get_rollout_state`). Each one was a
/// separate RPC round trip and could be served from a different ledger. A
/// snapshot is assembled inside a single invocation, so every field describes
/// the same ledger, recorded in `ledger`.
///
/// `anomaly` is the field to read first in an incident. It needs no context
/// to interpret, and it is also the first key in the encoded `ScMap`
/// (Soroban sorts struct fields by name).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HealthSnapshot {
    /// `true` when the contract is degraded, unhealthy, or stalled. Same value
    /// as `detect_anomaly`.
    pub anomaly: bool,
    /// Alert thresholds in effect (`get_alert_config`).
    pub alert_config: AlertConfig,
    /// Error rate in basis points derived from `metrics`.
    pub error_bps: u32,
    /// Ledger sequence at which this snapshot was taken.
    pub ledger: u32,
    /// Raw counters and pause flag (`get_health_metrics`).
    pub metrics: HealthMetrics,
    /// Canary / rollback state (`get_rollout_state`). `phase` is `RolledBack`
    /// once a rollback has fired; `rollback_error_bps` is the armed trigger.
    pub rollout: RolloutState,
    /// Published SLA targets (`get_sla_targets`).
    pub sla_targets: SlaTargets,
    /// `true` when no activity has been recorded for `alert_config.stall_ledgers`.
    pub stalled: bool,
    /// Classification against `alert_config`.
    pub status: HealthStatus,
}

/// Build a [`HealthSnapshot`] for the current ledger.
///
/// Strictly read-only. Unlike [`health_check`], it emits no `hlth_alrt`
/// event, does not touch the alert cooldown, and never triggers an automatic
/// rollback. It is therefore safe to simulate as often as a monitor likes.
pub fn health_snapshot(env: &Env) -> HealthSnapshot {
    let metrics = get_metrics(env);
    let alert_config = get_alert_config(env);
    let (status, anomaly) = classify(env, &metrics, &alert_config);
    HealthSnapshot {
        anomaly,
        error_bps: error_bps(&metrics),
        stalled: is_stalled(env, &metrics, &alert_config),
        ledger: env.ledger().sequence(),
        rollout: rollout::get_state(env),
        sla_targets: sla_targets(),
        status,
        metrics,
        alert_config,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Events as _, Ledger as _};
    use soroban_sdk::{contract, Address};

    #[contract]
    struct HealthHost;

    fn setup() -> (Env, Address) {
        let env = Env::default();
        let id = env.register_contract(None, HealthHost);
        (env, id)
    }

    #[test]
    fn snapshot_of_fresh_contract_is_healthy() {
        let (env, id) = setup();
        env.as_contract(&id, || {
            let snap = health_snapshot(&env);
            assert!(!snap.anomaly);
            assert_eq!(snap.status, HealthStatus::Healthy);
            assert_eq!(snap.error_bps, 0);
            assert!(!snap.stalled);
            assert_eq!(snap.sla_targets, sla_targets());
            assert_eq!(snap.alert_config, default_alert_config());
            assert_eq!(snap.rollout, rollout::get_state(&env));
            assert_eq!(snap.ledger, env.ledger().sequence());
        });
    }

    #[test]
    fn snapshot_agrees_with_individual_getters() {
        let (env, id) = setup();
        env.as_contract(&id, || {
            for _ in 0..9 {
                record_ok(&env);
            }
            record_error(&env); // 10% error rate -> Unhealthy under defaults
            let snap = health_snapshot(&env);
            assert_eq!(snap.anomaly, detect_anomaly(&env));
            assert!(snap.anomaly);
            assert_eq!(snap.status, HealthStatus::Unhealthy);
            assert_eq!(snap.metrics, get_metrics(&env));
            assert_eq!(snap.error_bps, 1_000);
            assert_eq!(snap.alert_config, get_alert_config(&env));
        });
    }

    #[test]
    fn snapshot_reports_stall() {
        let (env, id) = setup();
        env.ledger().with_mut(|l| l.sequence_number = 100);
        env.as_contract(&id, || record_ok(&env));
        env.ledger()
            .with_mut(|l| l.sequence_number = 100 + DEFAULT_STALL_LEDGERS);
        env.as_contract(&id, || {
            let snap = health_snapshot(&env);
            assert!(snap.stalled);
            assert!(snap.anomaly);
            assert_eq!(snap.status, HealthStatus::Degraded);
            assert_eq!(snap.ledger, 100 + DEFAULT_STALL_LEDGERS);
        });
    }

    #[test]
    fn snapshot_is_read_only() {
        let (env, id) = setup();
        env.as_contract(&id, || {
            record_error(&env); // anomaly with alerting enabled
        });
        let before = env.events().all().len();
        env.as_contract(&id, || {
            let snap = health_snapshot(&env);
            assert!(snap.anomaly);
            // No alert cooldown written, unlike health_check.
            let last: Option<u32> = env.storage().instance().get(&HealthKey::LastAlertLedger);
            assert_eq!(last, None);
        });
        assert_eq!(env.events().all().len(), before);
    }
}
