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
/// Volume-based fee tier (#690). A tier applies when the payer's cumulative
/// volume is at least `min_volume`. Tiers are stored sorted ascending by
/// `min_volume`; a volume matches the *largest* tier whose threshold it meets.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeTier {
    /// Minimum cumulative volume (in the fee token's base units) to qualify.
    pub min_volume: i128,
    /// Fee in basis points applied while this tier is active.
    pub fee_bps: u32,
}

/// Promotional fee period (#690). While the current ledger lies in the
/// `[start_ledger, end_ledger]` window, the promotional fee overrides the base
/// and tier fees.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Promotion {
    pub start_ledger: u32,
    pub end_ledger: u32,
    pub fee_bps: u32,
}

/// Referral fee sharing (#690). A share of the platform fee is redirected to a
/// referrer when one is attached to a fee computation.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferralConfig {
    /// Basis points of the platform fee shared with the referrer.
    pub bps: u32,
}

/// Result of a fee computation, giving integrators a single cross-contract
/// call to price an operation (#690).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeBreakdown {
    /// Effective fee applied (basis points) after tiers/promotions/clamps.
    pub effective_fee_bps: u32,
    /// Gross amount the fee is computed on.
    pub amount: i128,
    /// Total fee (`amount * effective_fee_bps / 10000`).
    pub fee: i128,
    /// Amount the fee is charged on, minus the fee.
    pub payout: i128,
    /// Portion of `fee` shared with the referrer (0 when none).
    pub referral_fee: i128,
    /// Portion of `fee` retained by the platform.
    pub platform_fee: i128,
}
