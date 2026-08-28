//! Gradual rollout of new contract versions (closes #684).
//!
//! Supports:
//!
//! 1. **Canary deployments** — a canary contract ID plus a traffic share.
//! 2. **Feature flags** — named, admin-toggled booleans.
//! 3. **Traffic splitting** — sticky per-caller routing via SHA-256 of the
//!    caller address, compared against `canary_bps`.
//! 4. **Rollback triggers** — when health error-rate exceeds a threshold,
//!    `should_rollback` is true and `health_check` auto-disables the canary.
//! 5. **Manual rollback** — `trigger_rollback` zeroes canary traffic, disables
//!    flags, and pauses the contract.
//!
//! Procedures are documented in `docs/DEPLOY.md`.

use crate::health;
use soroban_sdk::{
    contracttype, symbol_short, xdr::ToXdr, Address, Env, Symbol, Vec,
};

const BPS_DENOM: u32 = 10_000;
/// Default error-rate (bps) that arms an automatic canary rollback.
pub const DEFAULT_ROLLBACK_ERROR_BPS: u32 = 500;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RolloutKey {
    Phase,
    CanaryBps,
    Canary,
    Stable,
    RollbackErrorBps,
    Flag(Symbol),
    FlagIndex,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RolloutPhase {
    /// No canary traffic; flags may still be used independently.
    Off = 0,
    /// A fraction of callers are sent to the canary contract.
    Canary = 1,
    /// All traffic is on the new version (`canary_bps == 10000`).
    Full = 2,
    /// Canary has been disabled after a trigger or manual rollback.
    RolledBack = 3,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RolloutState {
    pub phase: RolloutPhase,
    pub canary_bps: u32,
    pub canary: Option<Address>,
    pub stable: Option<Address>,
    pub rollback_error_bps: u32,
    pub flag_count: u32,
}

fn flag_index(env: &Env) -> Vec<Symbol> {
    env.storage()
        .instance()
        .get(&RolloutKey::FlagIndex)
        .unwrap_or_else(|| Vec::new(env))
}

fn save_flag_index(env: &Env, index: &Vec<Symbol>) {
    env.storage()
        .instance()
        .set(&RolloutKey::FlagIndex, index);
}

pub fn get_canary_bps(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&RolloutKey::CanaryBps)
        .unwrap_or(0)
}

pub fn get_phase(env: &Env) -> RolloutPhase {
    env.storage()
        .instance()
        .get(&RolloutKey::Phase)
        .unwrap_or(RolloutPhase::Off)
}

pub fn get_rollback_error_bps(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&RolloutKey::RollbackErrorBps)
        .unwrap_or(DEFAULT_ROLLBACK_ERROR_BPS)
}

pub fn get_state(env: &Env) -> RolloutState {
    RolloutState {
        phase: get_phase(env),
        canary_bps: get_canary_bps(env),
        canary: env.storage().instance().get(&RolloutKey::Canary),
        stable: env.storage().instance().get(&RolloutKey::Stable),
        rollback_error_bps: get_rollback_error_bps(env),
        flag_count: flag_index(env).len(),
    }
}

fn phase_from_bps(bps: u32) -> RolloutPhase {
    if bps == 0 {
        RolloutPhase::Off
    } else if bps >= BPS_DENOM {
        RolloutPhase::Full
    } else {
        RolloutPhase::Canary
    }
}

/// Register canary/stable contract IDs and the share of traffic sent to canary.
pub fn set_canary_deployment(
    env: &Env,
    canary: Address,
    stable: Address,
    canary_bps: u32,
) {
    if canary_bps > BPS_DENOM {
        panic!("canary_bps exceeds 10000");
    }
    env.storage().instance().set(&RolloutKey::Canary, &canary);
    env.storage().instance().set(&RolloutKey::Stable, &stable);
    env.storage()
        .instance()
        .set(&RolloutKey::CanaryBps, &canary_bps);
    let phase = phase_from_bps(canary_bps);
    env.storage().instance().set(&RolloutKey::Phase, &phase);
    env.events()
        .publish((symbol_short!("canary"),), (canary, stable, canary_bps));
}

pub fn set_canary_bps(env: &Env, canary_bps: u32) {
    if canary_bps > BPS_DENOM {
        panic!("canary_bps exceeds 10000");
    }
    env.storage()
        .instance()
        .set(&RolloutKey::CanaryBps, &canary_bps);
    env.storage()
        .instance()
        .set(&RolloutKey::Phase, &phase_from_bps(canary_bps));
}

/// Sticky traffic split: SHA-256(caller) mod 10000 < canary_bps.
///
/// Returns `false` when the rollout has been rolled back or the share is 0.
pub fn route_to_canary(env: &Env, caller: &Address) -> bool {
    if get_phase(env) == RolloutPhase::RolledBack {
        return false;
    }
    let canary_bps = get_canary_bps(env);
    if canary_bps == 0 {
        return false;
    }
    if canary_bps >= BPS_DENOM {
        return true;
    }
    let digest = env.crypto().sha256(&caller.clone().to_xdr(env));
    let bytes = digest.to_array();
    let bucket = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) % BPS_DENOM;
    bucket < canary_bps
}

pub fn set_feature_flag(env: &Env, flag: &Symbol, enabled: bool) {
    env.storage()
        .instance()
        .set(&RolloutKey::Flag(flag.clone()), &enabled);
    let mut index = flag_index(env);
    let mut found = false;
    for existing in index.iter() {
        if existing == *flag {
            found = true;
            break;
        }
    }
    if !found {
        index.push_back(flag.clone());
        save_flag_index(env, &index);
    }
    env.events()
        .publish((symbol_short!("feat_flg"),), (flag.clone(), enabled));
}

pub fn is_feature_enabled(env: &Env, flag: &Symbol) -> bool {
    if get_phase(env) == RolloutPhase::RolledBack {
        return false;
    }
    env.storage()
        .instance()
        .get(&RolloutKey::Flag(flag.clone()))
        .unwrap_or(false)
}

pub fn set_rollback_trigger(env: &Env, error_bps: u32) {
    if error_bps == 0 || error_bps > BPS_DENOM {
        panic!("invalid rollback trigger");
    }
    env.storage()
        .instance()
        .set(&RolloutKey::RollbackErrorBps, &error_bps);
    env.events()
        .publish((symbol_short!("rb_trig"),), error_bps);
}

/// `true` when health error-rate meets the rollback trigger, or phase is already RolledBack.
pub fn should_rollback(env: &Env) -> bool {
    if get_phase(env) == RolloutPhase::RolledBack {
        return true;
    }
    let metrics = health::get_metrics(env);
    health::error_bps(&metrics) >= get_rollback_error_bps(env)
        && (metrics.ok_count + metrics.error_count) > 0
}

fn disable_all_flags(env: &Env) {
    let index = flag_index(env);
    for flag in index.iter() {
        env.storage()
            .instance()
            .set(&RolloutKey::Flag(flag.clone()), &false);
    }
}

fn apply_rollback(env: &Env) {
    env.storage()
        .instance()
        .set(&RolloutKey::CanaryBps, &0_u32);
    env.storage()
        .instance()
        .set(&RolloutKey::Phase, &RolloutPhase::RolledBack);
    disable_all_flags(env);
    env.events()
        .publish((symbol_short!("rollback"),), env.ledger().sequence());
}

/// Automatic rollback used by health checks when the trigger fires.
pub fn maybe_auto_rollback(env: &Env) -> bool {
    if get_phase(env) == RolloutPhase::RolledBack {
        return true;
    }
    if get_canary_bps(env) == 0 && flag_index(env).is_empty() {
        return false;
    }
    if should_rollback(env) {
        apply_rollback(env);
        return true;
    }
    false
}

/// Admin-initiated rollback: disable canary + flags and pause the contract.
/// Caller must already have authorized `admin` (the contract wrapper calls
/// `require_auth` once to avoid a double-auth HostError).
pub fn trigger_rollback(env: &Env, admin: &Address) {
    apply_rollback(env);
    env.storage()
        .instance()
        .set(&crate::pause::PauseDataKey::Paused, &true);
    env.events().publish(
        (soroban_sdk::Symbol::new(env, "contract_paused"),),
        crate::pause::ContractPausedEvent { admin: admin.clone() },
    );
}
