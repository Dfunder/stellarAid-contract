//! Agency account type, artist roster and revenue-split tracking (closes #609).

use soroban_sdk::{contracttype, Address, String};

use crate::errors::AgreementError;

pub const TOTAL_BPS: u32 = 10_000;

/// Upper bound on a single `distribute_batch` call, to keep the payout loop
/// inside a predictable resource envelope.
pub const MAX_BATCH: u32 = 25;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgencyProfile {
    pub agency: Address,
    pub name: String,
    /// Split applied to a newly rostered artist unless overridden.
    pub default_split_bps: u32,
    pub artist_count: u32,
    pub created_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RosterEntry {
    pub agency: Address,
    pub artist: Address,
    /// The agency's cut of this artist's gross earnings, in bps.
    pub split_bps: u32,
    pub joined_ledger: u32,
    pub commissions: u32,
    pub gross_distributed: i128,
    pub agency_revenue: i128,
    pub artist_payouts: i128,
}

/// Rolled-up agency figures. Kept as running totals rather than recomputed from
/// the roster so a read stays O(1) as the roster grows.
#[contracttype]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AgencyAnalytics {
    pub artist_count: u32,
    pub commissions: u32,
    pub commission_budget: i128,
    pub batches: u32,
    pub gross_distributed: i128,
    pub agency_revenue: i128,
    pub artist_payouts: i128,
}

/// One line of a batch payout: the gross owed to an artist before the agency
/// takes its rostered cut.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchPayment {
    pub artist: Address,
    pub gross_usdc: i128,
}

pub fn validate_split_bps(split_bps: u32) -> Result<(), AgreementError> {
    if split_bps > TOTAL_BPS {
        return Err(AgreementError::InvalidSplitBps);
    }
    Ok(())
}

/// Split a gross payment into (agency cut, artist net).
pub fn split_payment(gross_usdc: i128, split_bps: u32) -> Result<(i128, i128), AgreementError> {
    if gross_usdc <= 0 {
        return Err(AgreementError::InvalidAmount);
    }
    let agency_cut = gross_usdc
        .checked_mul(split_bps as i128)
        .ok_or(AgreementError::ArithmeticOverflow)?
        / TOTAL_BPS as i128;
    Ok((agency_cut, gross_usdc - agency_cut))
}
