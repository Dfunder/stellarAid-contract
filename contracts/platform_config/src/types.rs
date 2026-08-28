use soroban_sdk::{contracttype, Address};

#[contracttype]
#[derive(Clone, Debug)]
pub struct PlatformConfig {
    pub admin: Address,
    pub fee_bps: u32,
    pub platform_wallet: Address,
    pub usdc_token: Address,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct FeeTokenMetadata {
    pub name: soroban_sdk::String,
    pub symbol: soroban_sdk::String,
    pub decimal: u32,
    pub min_fee_bps: u32,
    pub max_fee_bps: u32,
}

/// Logical deployment environment used to namespace registered addresses so a
/// single config contract can serve both test and production deploys (#662).
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressEnvironment {
    Production,
    Test,
}

/// A named dependency registered in the address registry (#662).
#[contracttype]
#[derive(Clone, Debug)]
pub struct RegistryEntry {
    pub env: AddressEnvironment,
    pub name: soroban_sdk::Symbol,
    pub address: Address,
}

/// A cached registry resolution, tagged with the ledger it was resolved at so
/// stale entries can be detected and refreshed (#662).
#[contracttype]
#[derive(Clone, Debug)]
pub struct ResolutionCacheEntry {
    pub address: Address,
    pub resolved_ledger: u32,
}
