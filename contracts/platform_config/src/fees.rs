//! Advanced fee calculation logic (#690).
//!
//! The flat `fee_bps` baseline is resolved against *volume tiers*, and can be
//! overridden by an active *promotional period*. The effective fee is then
//! clamped to the token's `[min_fee_bps, max_fee_bps]` bounds. A configured
//! *referral share* splits the resulting fee between the referrer and the
//! platform wallet.
//!
//! Pure, checked math only — overflows return [`FeeComputationError`] so the
//! transaction is rejected cleanly instead of silently trashing a fee.

use soroban_sdk::Vec;

use crate::types::{FeeBreakdown, FeeTier, Promotion, ReferralConfig};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FeeComputationError {
    ArithmeticOverflow,
}

/// Resolves the effective fee in basis points.
///
/// Priority: active promotion overrides tiers overrides the base fee; then the
/// result is clamped into `[min_fee_bps, max_fee_bps]`.
pub fn resolve_effective_fee_bps(
    base_fee_bps: u32,
    tiers: &Vec<FeeTier>,
    promotion: Option<&Promotion>,
    volume: i128,
    current_ledger: u32,
    min_fee_bps: u32,
    max_fee_bps: u32,
) -> u32 {
    let mut effective = base_fee_bps;
    if let Some(p) = promotion {
        if p.start_ledger <= current_ledger && current_ledger <= p.end_ledger {
            effective = p.fee_bps;
        } else {
            effective = tier_fee(tiers, volume).unwrap_or(effective);
        }
    } else {
        effective = tier_fee(tiers, volume).unwrap_or(effective);
    }
    effective.clamp(min_fee_bps, max_fee_bps)
}

/// Picks the fee for the largest tier whose `min_volume` threshold is met.
fn tier_fee(tiers: &Vec<FeeTier>, volume: i128) -> Option<u32> {
    let mut matched = None;
    for t in tiers.iter() {
        if volume >= t.min_volume {
            matched = Some(t.fee_bps);
        } else {
            break; // tiers are sorted ascending by min_volume
        }
    }
    matched
}

/// Full fee computation, mirrored by the `compute_fees` contract entry point.
pub fn compute(
    base_fee_bps: u32,
    tiers: &Vec<FeeTier>,
    promotion: Option<&Promotion>,
    referral: Option<&ReferralConfig>,
    volume: i128,
    current_ledger: u32,
    amount: i128,
    min_fee_bps: u32,
    max_fee_bps: u32,
) -> Result<FeeBreakdown, FeeComputationError> {
    if amount < 0 {
        return Err(FeeComputationError::ArithmeticOverflow);
    }
    let effective_fee_bps =
        resolve_effective_fee_bps(base_fee_bps, tiers, promotion, volume, current_ledger, min_fee_bps, max_fee_bps);
    let fee = amount
        .checked_mul(effective_fee_bps as i128)
        .map(|v| v / 10_000)
        .ok_or(FeeComputationError::ArithmeticOverflow)?;
    let payout = amount
        .checked_sub(fee)
        .ok_or(FeeComputationError::ArithmeticOverflow)?;
    let (platform_fee, referral_fee) = match referral {
        Some(cfg) if cfg.bps > 0 => {
            let rf = fee
                .checked_mul(cfg.bps as i128)
                .map(|v| v / 10_000)
                .ok_or(FeeComputationError::ArithmeticOverflow)?;
            let pf = fee
                .checked_sub(rf)
                .ok_or(FeeComputationError::ArithmeticOverflow)?;
            (pf, rf)
        }
        _ => (fee, 0),
    };
    Ok(FeeBreakdown {
        effective_fee_bps,
        amount,
        fee,
        payout,
        referral_fee,
        platform_fee,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    fn tiers(env: &Env) -> Vec<FeeTier> {
        soroban_sdk::vec![
            env,
            FeeTier { min_volume: 0, fee_bps: 500 },
            FeeTier { min_volume: 50_000, fee_bps: 400 },
            FeeTier { min_volume: 200_000, fee_bps: 250 },
        ]
    }

    #[test]
    fn base_fee_applies_below_thresholds() {
        let env = Env::default();
        assert_eq!(
            resolve_effective_fee_bps(500, &tiers(&env), None, 10_000, 1, 0, 1000),
            500
        );
    }

    #[test]
    fn volume_tiers_lower_the_fee() {
        let env = Env::default();
        // 50k volume -> second tier.
        assert_eq!(
            resolve_effective_fee_bps(500, &tiers(&env), None, 60_000, 1, 0, 1000),
            400
        );
        // 300k volume -> third tier.
        assert_eq!(
            resolve_effective_fee_bps(500, &tiers(&env), None, 300_000, 1, 0, 1000),
            250
        );
    }

    #[test]
    fn promotion_overrides_tiers_only_when_active() {
        let env = Env::default();
        let promo = Promotion { start_ledger: 100, end_ledger: 200, fee_bps: 100 };
        // In-window: promo applies even at high volume.
        assert_eq!(
            resolve_effective_fee_bps(500, &tiers(&env), Some(&promo), 300_000, 150, 0, 1000),
            100
        );
        // Outside window: tiers apply.
        assert_eq!(
            resolve_effective_fee_bps(500, &tiers(&env), Some(&promo), 300_000, 250, 0, 1000),
            250
        );
    }

    #[test]
    fn clamps_to_token_bounds() {
        let env = Env::default();
        // Promo of 100 bps is below the 300 bps floor -> clamped up.
        let promo = Promotion { start_ledger: 0, end_ledger: 1000, fee_bps: 100 };
        assert_eq!(
            resolve_effective_fee_bps(500, &tiers(&env), Some(&promo), 0, 5, 300, 1000),
            300
        );
        // Base 2000 bps (20%) above the 1000 bps cap -> clamped down; no tiers
        // configured, so the base fee is what gets clamped.
        let none: Vec<FeeTier> = soroban_sdk::Vec::new(&env);
        assert_eq!(
            resolve_effective_fee_bps(2000, &none, None, 0, 5, 0, 1000),
            1000
        );
    }

    #[test]
    fn compute_splits_referral_share() {
        let env = Env::default();
        let referral = ReferralConfig { bps: 2000 }; // 20% of the platform fee
        let b = compute(
            500,
            &tiers(&env),
            None,
            Some(&referral),
            10_000,
            1,
            100_000,
            0,
            1000,
        )
        .unwrap();
        assert_eq!(b.effective_fee_bps, 500);
        assert_eq!(b.fee, 5_000);
        assert_eq!(b.payout, 95_000);
        assert_eq!(b.referral_fee, 1_000);
        assert_eq!(b.platform_fee, 4_000);
    }
}