//! Platform Configuration Contract
//!
//! Protocol-wide parameters, fee governance, and admin authority delegation.
//! Architecture Decision: [ADR-0005](../../docs/ADRs/0005-platform-fee-and-revenue-distribution.md)

#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Symbol};

pub mod errors;
pub mod storage;
pub mod types;

use errors::ConfigError;
use storage::*;
use types::{
    AddressEnvironment, FeeTokenMetadata, PlatformConfig, RegistryEntry, ResolutionCacheEntry,
};

include!("../../semver_types.rs");

#[contract]
pub struct PlatformConfigContract;

#[contractimpl]
impl PlatformConfigContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        fee_bps: u32,
        platform_wallet: Address,
        usdc_token: Address,
    ) -> Result<(), ConfigError> {
        if is_initialized(&env) {
            return Err(ConfigError::AlreadyInitialized);
        }
        if fee_bps > 1000 {
            return Err(ConfigError::InvalidFeeBps);
        }
        set_admin(&env, &admin);
        set_fee_bps_val(&env, fee_bps);
        set_platform_wallet(&env, &platform_wallet);
        set_usdc_token(&env, &usdc_token);
        env.events()
            .publish((symbol_short!("init"),), (admin.clone(), fee_bps));
        Ok(())
    }

    impl_semver_queries!();

    pub fn get_config(env: Env) -> PlatformConfig {
        PlatformConfig {
            admin: get_admin(&env),
            fee_bps: get_fee_bps(&env),
            platform_wallet: get_platform_wallet(&env),
            usdc_token: get_usdc_token(&env),
        }
    }

    pub fn set_fee_bps(env: Env, fee_bps: u32) -> Result<(), ConfigError> {
        let admin = get_admin(&env);
        admin.require_auth();
        if fee_bps > 1000 {
            return Err(ConfigError::InvalidFeeBps);
        }
        let old_fee = get_fee_bps(&env);
        set_fee_bps_val(&env, fee_bps);
        env.events()
            .publish((symbol_short!("feeupdtd"),), (old_fee, fee_bps));
        Ok(())
    }

    pub fn set_platform_wallet(env: Env, platform_wallet: Address) -> Result<(), ConfigError> {
        let admin = get_admin(&env);
        admin.require_auth();
        set_platform_wallet(&env, &platform_wallet);
        Ok(())
    }

    pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), ConfigError> {
        let admin = get_admin(&env);
        admin.require_auth();
        set_pending_admin(&env, &new_admin);
        env.events()
            .publish((symbol_short!("admprosd"),), new_admin);
        Ok(())
    }

    pub fn accept_admin(env: Env) -> Result<(), ConfigError> {
        let pending = get_pending_admin(&env).ok_or(ConfigError::NoPendingAdmin)?;
        pending.require_auth();
        set_admin(&env, &pending);
        env.events().publish((symbol_short!("admtxfrd"),), pending);
        Ok(())
    }

    pub fn set_token_metadata(
        env: Env,
        name: soroban_sdk::String,
        symbol: soroban_sdk::String,
        decimal: u32,
        min_fee_bps: u32,
        max_fee_bps: u32,
    ) -> Result<(), ConfigError> {
        let admin = get_admin(&env);
        admin.require_auth();
        if max_fee_bps > 1000 {
            return Err(ConfigError::InvalidFeeBps);
        }
        let meta = FeeTokenMetadata {
            name,
            symbol,
            decimal,
            min_fee_bps,
            max_fee_bps,
        };
        set_fee_token_metadata(&env, &meta);
        env.events().publish((symbol_short!("tkmeta"),), ());
        Ok(())
    }

    pub fn get_token_metadata(env: Env) -> FeeTokenMetadata {
        get_fee_token_metadata(&env)
    }

    // ── Address registry + resolution caching (#662) ──────────────────────
    /// Register (or overwrite) the contract address injected for `name` in the
    /// given environment. Admin-only. Invalidates the resolution cache for that
    /// key so a subsequent `resolve_*` observes the new address immediately.
    pub fn register_address(
        env: Env,
        e: AddressEnvironment,
        name: Symbol,
        address: Address,
    ) -> Result<(), ConfigError> {
        let admin = get_admin(&env);
        admin.require_auth();
        set_registered_address(&env, &e, &name, &address);
        remove_resolution_cache(&env, &e, &name);
        env.events()
            .publish((symbol_short!("addrreg"), name), (e, address));
        Ok(())
    }

    /// Remove a registered address. Admin-only. Unknown keys fail instead of
    /// silently no-opping so callers cannot mix up environments.
    pub fn unregister_address(
        env: Env,
        e: AddressEnvironment,
        name: Symbol,
    ) -> Result<(), ConfigError> {
        let admin = get_admin(&env);
        admin.require_auth();
        if get_registered_address(&env, &e, &name).is_none() {
            return Err(ConfigError::AddressNotRegistered);
        }
        remove_registered_address(&env, &e, &name);
        remove_resolution_cache(&env, &e, &name);
        Ok(())
    }

    /// Look up a registered address without touching the cache.
    pub fn get_registered_address(
        env: Env,
        e: AddressEnvironment,
        name: Symbol,
    ) -> Result<Address, ConfigError> {
        get_registered_address(&env, &e, &name).ok_or(ConfigError::AddressNotRegistered)
    }

    /// Full registry entry (env, name, address) for tooling/audits.
    pub fn registry_entry(
        env: Env,
        e: AddressEnvironment,
        name: Symbol,
    ) -> Result<RegistryEntry, ConfigError> {
        let address =
            get_registered_address(&env, &e, &name).ok_or(ConfigError::AddressNotRegistered)?;
        Ok(RegistryEntry { env: e, name, address })
    }

    /// Set the active environment so `resolve_for_environment` can be used by
    /// contracts that want to ignore the environment dimension. Admin-only.
    pub fn set_environment(env: Env, e: AddressEnvironment) -> Result<(), ConfigError> {
        let admin = get_admin(&env);
        admin.require_auth();
        set_active_environment(&env, e);
        Ok(())
    }

    pub fn get_environment(env: Env) -> AddressEnvironment {
        get_active_environment(&env)
    }

    /// Resolve `name` in environment `e`, honoring a fresh resolution cache
    /// entry and populating the cache on a registry miss.
    pub fn resolve_address(
        env: Env,
        e: AddressEnvironment,
        name: Symbol,
    ) -> Result<Address, ConfigError> {
        if let Some(cached) = get_resolution_cache(&env, &e, &name) {
            let now = env.ledger().sequence();
            if now >= cached.resolved_ledger
                && now - cached.resolved_ledger <= RESOLUTION_CACHE_TTL_LEDGERS
            {
                return Ok(cached.address);
            }
        }
        let addr = get_registered_address(&env, &e, &name).ok_or(ConfigError::AddressNotRegistered)?;
        set_resolution_cache(
            &env,
            &e,
            &name,
            &ResolutionCacheEntry {
                address: addr.clone(),
                resolved_ledger: env.ledger().sequence(),
            },
        );
        Ok(addr)
    }

    /// Resolve using the currently active environment (test vs. production).
    pub fn resolve_for_environment(env: Env, name: Symbol) -> Result<Address, ConfigError> {
        let e = get_active_environment(&env);
        Self::resolve_address(env, e, name)
    }

    /// Inspect the resolution cache for `(e, name)`.
    pub fn resolution_cache(
        env: Env,
        e: AddressEnvironment,
        name: Symbol,
    ) -> Result<ResolutionCacheEntry, ConfigError> {
        get_resolution_cache(&env, &e, &name).ok_or(ConfigError::AddressNotRegistered)
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


#[cfg(test)]
mod tests;
