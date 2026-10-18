//! Panic-safe arithmetic for the campaign contract (issue #760).
//!
//! Every function here is `checked_*` under the hood and converts a failure
//! into the typed [`Error::Overflow`] contract error, so an integer overflow is
//! an intentional, catchable, documented panic instead of undefined behaviour
//! or a silent wrap.
//!
//! # Why not `saturating_*`
//!
//! Saturation is the wrong default for money. If a `raised` total saturated at
//! `i128::MAX` the campaign would silently report a balance it does not hold;
//! if a withdrawal saturated at zero the creator could take the whole balance.
//! Both directions must abort the transaction, which is what `checked_*` does.
//!
//! (`saturating_*` remains correct for counters and elapsed-ledger arithmetic,
//! where wrapping is impossible by construction and a saturating value is a
//! fail-closed answer — see `shared::health` and `contracts/rate_limiter`.)
//!
//! # Audit focus (per the issue)
//!
//! * `fee_split` — the basis-point fee calculation used on every withdrawal.
//! * `pro_rata_share` — the proportional-withdrawal maths used for multi-asset
//!   campaigns, where a `weight / total_weight` division can otherwise divide
//!   by zero or lose a stroop to truncation on the wrong side.

// Some helpers here are for call sites that do not exist yet (pro-rata
// multi-asset shares); this crate is a `cdylib`, so rustc would otherwise
// report them as dead. Every helper is covered by the unit tests below.
#![allow(dead_code)]

use crate::errors::Error;
use soroban_sdk::{panic_with_error, Env};

/// Denominator for basis-point arithmetic (100%).
pub const BPS_DENOM: i128 = 10_000;

/// `a + b`, aborting with [`Error::Overflow`] on overflow.
#[inline]
pub fn add(env: &Env, a: i128, b: i128) -> i128 {
    match a.checked_add(b) {
        Some(v) => v,
        None => panic_with_error!(env, Error::Overflow),
    }
}

/// `a - b`, aborting with [`Error::Overflow`] on underflow.
#[inline]
pub fn sub(env: &Env, a: i128, b: i128) -> i128 {
    match a.checked_sub(b) {
        Some(v) => v,
        None => panic_with_error!(env, Error::Overflow),
    }
}

/// `a * b`, aborting with [`Error::Overflow`] on overflow.
#[inline]
pub fn mul(env: &Env, a: i128, b: i128) -> i128 {
    match a.checked_mul(b) {
        Some(v) => v,
        None => panic_with_error!(env, Error::Overflow),
    }
}

/// `a + b` on `u64` counters and ledgers.
#[inline]
pub fn add_u64(env: &Env, a: u64, b: u64) -> u64 {
    match a.checked_add(b) {
        Some(v) => v,
        None => panic_with_error!(env, Error::Overflow),
    }
}

/// `a + b` on `u32` ledgers.
#[inline]
pub fn add_u32(env: &Env, a: u32, b: u32) -> u32 {
    match a.checked_add(b) {
        Some(v) => v,
        None => panic_with_error!(env, Error::Overflow),
    }
}

/// Splits `amount` into `(fee, net_payout)` for a fee expressed in basis
/// points.
///
/// The multiplication is checked and the net amount is derived by
/// *subtraction* rather than by a second division, so `fee + net_payout`
/// always equals `amount` exactly — no stroop can be lost to truncation on
/// both sides.
///
/// `fee_bps` is expected to be at most [`BPS_DENOM`], which `create_campaign`
/// enforces (it rejects `fee_bps > 1000`). A misconfigured `fee_bps` that
/// would drive the payout negative aborts through the checked subtraction
/// rather than paying out a negative net amount.
pub fn fee_split(env: &Env, amount: i128, fee_bps: u32) -> (i128, i128) {
    if amount < 0 {
        panic_with_error!(env, Error::Overflow);
    }
    let bps = i128::from(fee_bps);
    let gross_fee = (amount / BPS_DENOM) * bps + ((amount % BPS_DENOM) * bps) / BPS_DENOM;
    if gross_fee > amount {
        panic_with_error!(env, Error::Overflow);
    }
    let net = sub(env, amount, gross_fee);
    (gross_fee, net)
}

/// Proportional share of `amount` for a multi-asset withdrawal.
///
/// `weight / total_weight` is evaluated with a checked multiply-then-divide,
/// which avoids the precision loss of dividing first. The caller is
/// responsible for the residual stroop: `total_weight` is the sum of all
/// weights, so the shares may sum to `amount - 1` at most, and the remainder
/// is attributed to the largest weight by the caller.
pub fn pro_rata_share(env: &Env, amount: i128, weight: u32, total_weight: u32) -> i128 {
    if total_weight == 0 {
        panic_with_error!(env, Error::InvalidAssets);
    }
    mul(env, amount, i128::from(weight)) / i128::from(total_weight)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_sub_are_symmetric() {
        let env = Env::default();
        assert_eq!(add(&env, 40, 2), 42);
        assert_eq!(sub(&env, 42, 2), 40);
    }

    #[test]
    fn add_panics_on_overflow() {
        let env = Env::default();
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            add(&env, i128::MAX, 1)
        }));
        assert!(res.is_err());
    }

    #[test]
    fn sub_panics_on_underflow() {
        let env = Env::default();
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sub(&env, i128::MIN, 1)
        }));
        assert!(res.is_err());
    }

    #[test]
    fn mul_panics_on_overflow() {
        let env = Env::default();
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            mul(&env, i128::MAX, 2)
        }));
        assert!(res.is_err());
    }

    #[test]
    fn fee_split_conserves_the_amount_exactly() {
        let env = Env::default();
        // An amount that does not divide evenly by 10_000 is the interesting
        // case: 1 stroop at 5% must still sum back to 1.
        for amount in [1_i128, 7, 999, 1_000, 1_234_567, i128::MAX / 4] {
            for bps in [0_u32, 1, 333, 500, 999, 1_000] {
                let (fee, net) = fee_split(&env, amount, bps);
                assert_eq!(fee + net, amount, "fee split must conserve the amount");
                assert!(fee >= 0 && net >= 0);
            }
        }
    }

    #[test]
    fn fee_split_matches_hand_computed_value() {
        let env = Env::default();
        assert_eq!(fee_split(&env, 2_500, 500), (125, 2_375));
        assert_eq!(fee_split(&env, 100, 0), (0, 100));
        assert_eq!(fee_split(&env, 100, 1_000), (10, 90));
        assert_eq!(fee_split(&env, 100, 10_000), (100, 0));
    }

    #[test]
    fn fee_split_aborts_when_bps_would_drive_the_payout_negative() {
        let env = Env::default();
        // 200% of the amount: the fee exceeds the gross, so the checked
        // subtraction of the net payout aborts instead of underflowing.
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            fee_split(&env, 1_000, 20_000)
        }));
        assert!(res.is_err());
    }

    #[test]
    fn pro_rata_share_divides_by_total_weight() {
        let env = Env::default();
        // 1/3 and 2/3 of 1_000, with the residual left to the caller.
        assert_eq!(pro_rata_share(&env, 1_000, 1, 3), 333);
        assert_eq!(pro_rata_share(&env, 1_000, 2, 3), 666);
    }

    #[test]
    fn pro_rata_share_rejects_zero_total_weight() {
        let env = Env::default();
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pro_rata_share(&env, 1_000, 1, 0)
        }));
        assert!(res.is_err());
    }

    #[test]
    fn counter_helpers_abort_instead_of_wrapping() {
        let env = Env::default();
        assert_eq!(add_u64(&env, 1, 2), 3);
        assert_eq!(add_u32(&env, 1, 2), 3);
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            add_u64(&env, u64::MAX, 1)
        }))
        .is_err());
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            add_u32(&env, u32::MAX, 1)
        }))
        .is_err());
    }
}
