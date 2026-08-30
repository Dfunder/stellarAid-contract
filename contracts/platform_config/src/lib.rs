//! Platform Configuration Contract
//!
//! Protocol-wide parameters, fee governance, and admin authority delegation.
//! Architecture Decision: [ADR-0005](../../docs/ADRs/0005-platform-fee-and-revenue-distribution.md)

#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env};

pub mod errors;
pub mod fees;
pub mod storage;
pub mod types;

use errors::ConfigError;
use storage::*;
use types::{FeeBreakdown, FeeTier, FeeTokenMetadata, PlatformConfig, Promotion, ReferralConfig};

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

    // ── Advanced fee structures (closes #690) ────────────────────────────
    // Entry points above (tiers, promotions, referral, volume) are written
    // directly in the `#[contractimpl]` block — SDK-21 does not export
    // macro-generated (`impl_semver_queries!`) functions, so they must be
    // explicit to be callable cross-contract.

    /// Add (or replace) a volume-based fee tier. Admin only.
    pub fn upsert_fee_tier(env: Env, admin: Address, tier: FeeTier) -> Result<bool, ConfigError> {
        admin.require_auth();
        let stored = get_admin(&env);
        if stored != admin {
            return Err(ConfigError::Unauthorized);
        }
        if tier.min_volume < 0 || tier.fee_bps > 1000 {
            return Err(ConfigError::InvalidTier);
        }
        let replaced = storage::upsert_fee_tier(&env, &tier);
        env.events().publish(
            (symbol_short!("tier"),), (tier.min_volume, tier.fee_bps),
        );
        Ok(replaced)
    }

    /// Remove the tier with the given volume threshold. Admin only.
    pub fn remove_fee_tier(env: Env, admin: Address, min_volume: i128) -> Result<bool, ConfigError> {
        admin.require_auth();
        let stored = get_admin(&env);
        if stored != admin {
            return Err(ConfigError::Unauthorized);
        }
        let removed = storage::remove_fee_tier(&env, min_volume);
        env.events().publish((symbol_short!("tierrm"),), min_volume);
        Ok(removed)
    }

    /// List all configured fee tiers, sorted ascending by volume threshold.
    pub fn get_fee_tiers(env: Env) -> soroban_sdk::Vec<FeeTier> {
        storage::get_fee_tiers(&env)
    }

    /// Configure a promotional fee period. Admin only.
    pub fn set_promotion(env: Env, admin: Address, promotion: Promotion) -> Result<(), ConfigError> {
        admin.require_auth();
        let stored = get_admin(&env);
        if stored != admin {
            return Err(ConfigError::Unauthorized);
        }
        if promotion.end_ledger < promotion.start_ledger || promotion.fee_bps > 1000 {
            return Err(ConfigError::InvalidPromotion);
        }
        storage::set_promotion(&env, &promotion);
        env.events().publish(
            (symbol_short!("promo"),),
            (promotion.start_ledger, promotion.end_ledger, promotion.fee_bps),
        );
        Ok(())
    }

    /// End the current promotional period, if any. Admin only.
    pub fn clear_promotion(env: Env, admin: Address) -> Result<(), ConfigError> {
        admin.require_auth();
        let stored = get_admin(&env);
        if stored != admin {
            return Err(ConfigError::Unauthorized);
        }
        storage::clear_promotion(&env);
        env.events().publish((symbol_short!("promo"), symbol_short!("clr")), ());
        Ok(())
    }

    /// Configure the referrer share of the platform fee (bps). Admin only.
    pub fn set_referral_config(env: Env, admin: Address, config: ReferralConfig) -> Result<(), ConfigError> {
        admin.require_auth();
        let stored = get_admin(&env);
        if stored != admin {
            return Err(ConfigError::Unauthorized);
        }
        if config.bps > 10_000 {
            return Err(ConfigError::InvalidReferralBps);
        }
        storage::set_referral_config(&env, &config);
        env.events().publish((symbol_short!("refcfg"),), config.bps);
        Ok(())
    }

    /// Read the configured referral share, if any.
    pub fn get_referral_config(env: Env) -> Option<ReferralConfig> {
        storage::get_referral_config(&env)
    }

    /// Record `amount` of volume for a payer; feeds volume-based discounts.
    /// Admin only (the escrow contract is expected to call this on release).
    pub fn record_volume(env: Env, admin: Address, payer: Address, amount: i128) -> Result<i128, ConfigError> {
        admin.require_auth();
        let stored = get_admin(&env);
        if stored != admin {
            return Err(ConfigError::Unauthorized);
        }
        if amount < 0 {
            return Err(ConfigError::InvalidFeeBps);
        }
        storage::record_volume(&env, &payer, amount);
        let total = storage::get_volume(&env, &payer);
        env.events().publish((symbol_short!("vol"),), (payer.clone(), amount, total));
        Ok(total)
    }

    /// Read a payer's cumulative volume.
    pub fn get_volume(env: Env, payer: Address) -> i128 {
        storage::get_volume(&env, &payer)
    }

    /// Resolve the effective fee (bps) for an operation, given the payer's
    /// cumulative volume, at the current ledger. Pure resolution — the same
    /// rules `compute_fees` applies to produce a full breakdown.
    pub fn resolve_effective_fee_bps(env: Env, volume: i128) -> u32 {
        fees::resolve_effective_fee_bps(
            get_fee_bps(&env),
            &storage::get_fee_tiers(&env),
            storage::get_promotion(&env).as_ref(),
            volume,
            env.ledger().sequence(),
            get_min_fee_bps(&env),
            get_max_fee_bps(&env),
        )
    }

    /// Full fee computation for an operation. Cross-contract friendly: escrow
    /// and other fee collectors can invoke this to price a payout, optionally
    /// charging `referrer` a share of the platform fee.
    pub fn compute_fees(
        env: Env,
        amount: i128,
        volume: i128,
        referrer: Option<Address>,
    ) -> Result<FeeBreakdown, ConfigError> {
        let referral = match referrer {
            Some(_) => storage::get_referral_config(&env),
            None => None,
        };
        fees::compute(
            get_fee_bps(&env),
            &storage::get_fee_tiers(&env),
            storage::get_promotion(&env).as_ref(),
            referral.as_ref(),
            volume,
            env.ledger().sequence(),
            amount,
            get_min_fee_bps(&env),
            get_max_fee_bps(&env),
        )
        .map_err(|_| ConfigError::InvalidFeeBps)
    }

    /// Whether a promotional period is active at the current ledger.
    pub fn is_promotion_active(env: Env) -> bool {
        match storage::get_promotion(&env) {
            Some(p) => {
                let now = env.ledger().sequence();
                p.start_ledger <= now && now <= p.end_ledger
            }
            None => false,
        }
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
