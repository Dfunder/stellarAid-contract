use soroban_sdk::{contracttype, Address, Env, Symbol};

use crate::types::{AddressEnvironment, FeeTokenMetadata, ResolutionCacheEntry};

/// Resolutions are considered fresh for this many ledgers (~5 days at 5s/ledger).
pub const RESOLUTION_CACHE_TTL_LEDGERS: u32 = 86_400;
use crate::types::{FeeTier, FeeTokenMetadata, Promotion, ReferralConfig};

#[contracttype]
pub enum DataKey {
    Admin,
    FeeBps,
    PlatformWallet,
    UsdcToken,
    PendingAdmin,
    TokenName,
    TokenSymbol,
    TokenDecimal,
    MinFeeBps,
    MaxFeeBps,
    /// Active deployment environment for unqualified resolutions.
    ActiveEnvironment,
    /// (env, name) -> Address
    RegistryEntry(AddressEnvironment, Symbol),
    /// (env, name) -> ResolutionCacheEntry
    ResolutionCache(AddressEnvironment, Symbol),
    FeeTiers,
    Promotion,
    ReferralConfig,
    Volume(Address),
}

/// Default TTL for per-payer volume records (~30 days at 6s/ledger).
pub const VOLUME_TTL_LEDGERS: u32 = 432_000;

pub fn get_admin(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Admin).unwrap()
}
pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}
pub fn get_fee_bps(env: &Env) -> u32 {
    env.storage().instance().get(&DataKey::FeeBps).unwrap()
}
pub fn set_fee_bps_val(env: &Env, fee_bps: u32) {
    env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
}
pub fn get_platform_wallet(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::PlatformWallet).unwrap()
}
pub fn set_platform_wallet(env: &Env, wallet: &Address) {
    env.storage().instance().set(&DataKey::PlatformWallet, wallet);
}
pub fn get_usdc_token(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::UsdcToken).unwrap()
}
pub fn set_usdc_token(env: &Env, token: &Address) {
    env.storage().instance().set(&DataKey::UsdcToken, token);
}
pub fn get_pending_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::PendingAdmin)
}
pub fn set_pending_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::PendingAdmin, admin);
}
pub fn is_initialized(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

pub fn get_token_name(env: &Env) -> soroban_sdk::String {
    env.storage().instance().get(&DataKey::TokenName).unwrap()
}
pub fn set_token_name(env: &Env, name: &soroban_sdk::String) {
    env.storage().instance().set(&DataKey::TokenName, name);
}
pub fn get_token_symbol(env: &Env) -> soroban_sdk::String {
    env.storage().instance().get(&DataKey::TokenSymbol).unwrap()
}
pub fn set_token_symbol(env: &Env, symbol: &soroban_sdk::String) {
    env.storage().instance().set(&DataKey::TokenSymbol, symbol);
}
pub fn get_token_decimal(env: &Env) -> u32 {
    env.storage().instance().get(&DataKey::TokenDecimal).unwrap()
}
pub fn set_token_decimal(env: &Env, decimal: u32) {
    env.storage().instance().set(&DataKey::TokenDecimal, &decimal);
}
pub fn get_min_fee_bps(env: &Env) -> u32 {
    env.storage().instance().get(&DataKey::MinFeeBps).unwrap_or(0)
}
pub fn set_min_fee_bps(env: &Env, bps: u32) {
    env.storage().instance().set(&DataKey::MinFeeBps, &bps);
}
pub fn get_max_fee_bps(env: &Env) -> u32 {
    env.storage().instance().get(&DataKey::MaxFeeBps).unwrap_or(1000)
}
pub fn set_max_fee_bps(env: &Env, bps: u32) {
    env.storage().instance().set(&DataKey::MaxFeeBps, &bps);
}
pub fn get_fee_token_metadata(env: &Env) -> FeeTokenMetadata {
    FeeTokenMetadata {
        name: get_token_name(env),
        symbol: get_token_symbol(env),
        decimal: get_token_decimal(env),
        min_fee_bps: get_min_fee_bps(env),
        max_fee_bps: get_max_fee_bps(env),
    }
}
pub fn set_fee_token_metadata(env: &Env, meta: &FeeTokenMetadata) {
    set_token_name(env, &meta.name);
    set_token_symbol(env, &meta.symbol);
    set_token_decimal(env, meta.decimal);
    set_min_fee_bps(env, meta.min_fee_bps);
    set_max_fee_bps(env, meta.max_fee_bps);
}

// ── Address registry + resolution cache (#662) ──────────────────────────────

pub fn get_active_environment(env: &Env) -> AddressEnvironment {
    env.storage()
        .instance()
        .get(&DataKey::ActiveEnvironment)
        .unwrap_or(AddressEnvironment::Production)
}
pub fn set_active_environment(env: &Env, e: AddressEnvironment) {
    env.storage().instance().set(&DataKey::ActiveEnvironment, &e);
}

pub fn get_registered_address(env: &Env, e: &AddressEnvironment, name: &Symbol) -> Option<Address> {
    env.storage()
        .instance()
        .get(&DataKey::RegistryEntry(e.clone(), name.clone()))
}
pub fn set_registered_address(env: &Env, e: &AddressEnvironment, name: &Symbol, address: &Address) {
    env.storage()
        .instance()
        .set(&DataKey::RegistryEntry(e.clone(), name.clone()), address);
}
pub fn remove_registered_address(env: &Env, e: &AddressEnvironment, name: &Symbol) {
    env.storage()
        .instance()
        .remove(&DataKey::RegistryEntry(e.clone(), name.clone()));
}

pub fn get_resolution_cache(
    env: &Env,
    e: &AddressEnvironment,
    name: &Symbol,
) -> Option<ResolutionCacheEntry> {
    env.storage()
        .instance()
        .get(&DataKey::ResolutionCache(e.clone(), name.clone()))
}
pub fn set_resolution_cache(env: &Env, e: &AddressEnvironment, name: &Symbol, entry: &ResolutionCacheEntry) {
    env.storage()
        .instance()
        .set(&DataKey::ResolutionCache(e.clone(), name.clone()), entry);
}
pub fn remove_resolution_cache(env: &Env, e: &AddressEnvironment, name: &Symbol) {
    env.storage()
        .instance()
        .remove(&DataKey::ResolutionCache(e.clone(), name.clone()));
// ── Advanced fee structures (#690) ─────────────────────────────────────────

/// All configured fee tiers, sorted ascending by `min_volume`.
pub fn get_fee_tiers(env: &Env) -> soroban_sdk::Vec<FeeTier> {
    env.storage()
        .instance()
        .get(&DataKey::FeeTiers)
        .unwrap_or_else(|| soroban_sdk::vec![env,])
}
pub fn set_fee_tiers(env: &Env, tiers: &soroban_sdk::Vec<FeeTier>) {
    env.storage().instance().set(&DataKey::FeeTiers, tiers);
}
/// Insert (or replace) a tier, keeping the list sorted ascending by volume.
/// Returns `true` when a tier was replaced.
pub fn upsert_fee_tier(env: &Env, tier: &FeeTier) -> bool {
    let mut replaced = false;
    let mut inserted = false;
    let mut out: soroban_sdk::Vec<FeeTier> = soroban_sdk::Vec::new(env);
    for t in get_fee_tiers(env).iter() {
        if t.min_volume == tier.min_volume {
            out.push_back(tier.clone());
            replaced = true;
            inserted = true;
        } else if !inserted && t.min_volume > tier.min_volume {
            out.push_back(tier.clone());
            inserted = true;
            out.push_back(t);
        } else {
            out.push_back(t);
        }
    }
    if !inserted {
        out.push_back(tier.clone());
    }
    set_fee_tiers(env, &out);
    replaced
}
/// Removes the tier with the given threshold, returning `true` when removed.
pub fn remove_fee_tier(env: &Env, min_volume: i128) -> bool {
    let mut found = false;
    let mut out: soroban_sdk::Vec<FeeTier> = soroban_sdk::Vec::new(env);
    for t in get_fee_tiers(env).iter() {
        if t.min_volume == min_volume {
            found = true;
        } else {
            out.push_back(t);
        }
    }
    if found {
        set_fee_tiers(env, &out);
    }
    found
}

pub fn get_promotion(env: &Env) -> Option<Promotion> {
    env.storage().instance().get(&DataKey::Promotion)
}
pub fn set_promotion(env: &Env, promotion: &Promotion) {
    env.storage().instance().set(&DataKey::Promotion, promotion);
}
pub fn clear_promotion(env: &Env) {
    env.storage().instance().remove(&DataKey::Promotion);
}

pub fn get_referral_config(env: &Env) -> Option<ReferralConfig> {
    env.storage().instance().get(&DataKey::ReferralConfig)
}
pub fn set_referral_config(env: &Env, config: &ReferralConfig) {
    env.storage().instance().set(&DataKey::ReferralConfig, config);
}

/// Cumulative volume recorded for a payer (persistent, TTL-managed).
pub fn get_volume(env: &Env, payer: &Address) -> i128 {
    env.storage().persistent().get(&DataKey::Volume(payer.clone())).unwrap_or(0)
}
/// Add `amount` to a payer's cumulative volume and renew its retention.
pub fn record_volume(env: &Env, payer: &Address, amount: i128) {
    let total = get_volume(env, payer).saturating_add(amount);
    let key = DataKey::Volume(payer.clone());
    env.storage().persistent().set(&key, &total);
    env.storage()
        .persistent()
        .extend_ttl(&key, VOLUME_TTL_LEDGERS, VOLUME_TTL_LEDGERS);
}
