//! Unit tests for the overflow-safe fee split (#588).
//!
//! Exercises [`crate::calculate_fee_split`] directly — including i128::MAX
//! scenarios — without requiring a deployed contract environment.

use crate::{calculate_fee_split, errors::EscrowError};

// ── Happy paths ─────────────────────────────────────────────────────────────

#[test]
fn fee_split_500bps() {
    let (fee, payout) = calculate_fee_split(10_000, 500).unwrap();
    assert_eq!(fee, 500);
    assert_eq!(payout, 9_500);
}

#[test]
fn fee_split_zero_bps() {
    let (fee, payout) = calculate_fee_split(50_000, 0).unwrap();
    assert_eq!(fee, 0);
    assert_eq!(payout, 50_000);
}

#[test]
fn fee_split_max_bps() {
    // 10_000 bps = 100 %: everything goes to the platform wallet.
    let (fee, payout) = calculate_fee_split(7_777, 10_000).unwrap();
    assert_eq!(fee, 7_777);
    assert_eq!(payout, 0);
}

#[test]
fn fee_split_one_unit_amount() {
    let (fee, payout) = calculate_fee_split(1, 2500).unwrap();
    assert_eq!(fee, 0); // integer division truncates
    assert_eq!(payout, 1);
}

#[test]
fn fee_and_payout_always_sum_to_amount() {
    let amount: i128 = 123_456_789;
    let (fee, payout) = calculate_fee_split(amount, 733).unwrap();
    assert_eq!(fee + payout, amount);
}

#[test]
fn fee_split_large_but_safe_amount() {
    // Largest amount whose product with 10_000 bps still fits in i128.
    let amount = i128::MAX / 10_000;
    let (fee, payout) = calculate_fee_split(amount, 10_000).unwrap();
    assert_eq!(fee, amount);
    assert_eq!(payout, 0);
}

// ── Overflow edge cases (#588 acceptance criteria) ──────────────────────────

#[test]
fn fee_split_i128_max_overflows() {
    let result = calculate_fee_split(i128::MAX, 500);
    assert_eq!(result.unwrap_err(), EscrowError::ArithmeticOverflow);
}

#[test]
fn fee_split_i128_max_zero_bps_is_safe() {
    // amount * 0 never overflows; full refund to the artist.
    let (fee, payout) = calculate_fee_split(i128::MAX, 0).unwrap();
    assert_eq!(fee, 0);
    assert_eq!(payout, i128::MAX);
}

#[test]
fn fee_split_just_below_overflow_boundary() {
    let boundary = i128::MAX / 500 + 1; // first amount that overflows at 500 bps
    assert_eq!(
        calculate_fee_split(boundary, 500).unwrap_err(),
        EscrowError::ArithmeticOverflow
    );
    // One unit lower is fine.
    let (fee, payout) = calculate_fee_split(boundary - 1, 500).unwrap();
    assert_eq!(fee + payout, boundary - 1);
}

#[test]
fn fee_split_overflow_error_code() {
    assert_eq!(EscrowError::ArithmeticOverflow as u32, 12);
}
