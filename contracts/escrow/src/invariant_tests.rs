extern crate std;
use soroban_sdk::Env;
use crate::invariants;
use crate::storage::{CommissionStatus, EscrowRecord};
use crate::errors::EscrowError;

fn make_record(env: &Env, amount: i128, released: i128, status: CommissionStatus) -> EscrowRecord {
    EscrowRecord {
        commission_id: soroban_sdk::Bytes::from_slice(env, b"test-id"),
        client: soroban_sdk::Address::generate(env),
        artist: soroban_sdk::Address::generate(env),
        amount,
        fee_bps: 500,
        status,
        created_ledger: 1,
        released_amount: released,
    }
}

#[test]
fn balance_consistency_passes_for_normal_record() {
    let env = Env::default();
    let r = make_record(&env, 100_000, 30_000, CommissionStatus::PartiallyReleased);
    assert!(invariants::verify_balance_consistency(&env, &r).is_ok());
}

#[test]
fn balance_consistency_rejects_over_release() {
    let env = Env::default();
    let r = make_record(&env, 100_000, 101_000, CommissionStatus::Released);
    let err = invariants::verify_balance_consistency(&env, &r).unwrap_err();
    assert_eq!(err, invariants::InvariantError::ReleasedExceedsAmount);
}

#[test]
fn balance_consistency_rejects_negative_release() {
    let env = Env::default();
    let r = make_record(&env, 100_000, -1, CommissionStatus::Locked);
    let err = invariants::verify_balance_consistency(&env, &r).unwrap_err();
    assert_eq!(err, invariants::InvariantError::ReleasedIsNegative);
}

#[test]
fn state_transition_valid_for_locked_to_released() {
    assert!(invariants::is_valid_transition(
        CommissionStatus::Locked,
        CommissionStatus::Released
    ));
}

#[test]
fn state_transition_invalid_for_released_to_locked() {
    assert!(!invariants::is_valid_transition(
        CommissionStatus::Released,
        CommissionStatus::Locked
    ));
}

#[test]
fn state_transition_valid_for_partially_released_to_released() {
    assert!(invariants::is_valid_transition(
        CommissionStatus::PartiallyReleased,
        CommissionStatus::Released
    ));
}

#[test]
fn state_transition_valid_for_disputed_to_refunded() {
    assert!(invariants::is_valid_transition(
        CommissionStatus::Disputed,
        CommissionStatus::Refunded
    ));
}

#[test]
fn state_transition_invalid_for_expired_to_released() {
    assert!(!invariants::is_valid_transition(
        CommissionStatus::Expired,
        CommissionStatus::Released
    ));
}

#[test]
fn validate_state_consistency_allows_valid_transition() {
    let env = Env::default();
    let before = make_record(&env, 100_000, 0, CommissionStatus::Locked);
    let after = make_record(&env, 100_000, 100_000, CommissionStatus::Released);
    assert!(invariants::validate_state_consistency(&env, &before, &after).is_ok());
}

#[test]
fn validate_state_consistency_rejects_invalid_transition() {
    let env = Env::default();
    let before = make_record(&env, 100_000, 0, CommissionStatus::Released);
    let after = make_record(&env, 100_000, 0, CommissionStatus::Locked);
    let err = invariants::validate_state_consistency(&env, &before, &after).unwrap_err();
    assert_eq!(err, invariants::InvariantError::InvalidStateTransition);
}

#[test]
fn validate_state_consistency_allows_same_status() {
    let env = Env::default();
    let r = make_record(&env, 100_000, 50_000, CommissionStatus::PartiallyReleased);
    assert!(invariants::validate_state_consistency(&env, &r, &r).is_ok());
}

#[test]
fn fee_split_verification_passes_for_correct_split() {
    assert!(invariants::verify_fee_split(100_000, 5_000, 95_000).is_ok());
}

#[test]
fn fee_split_verification_fails_for_incorrect_split() {
    let result = invariants::verify_fee_split(100_000, 5_001, 95_000);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), EscrowError::InvalidSplit);
}

#[test]
fn compute_fee_correct_for_500_bps() {
    let (fee, payout) = invariants::compute_fee(100_000, 500).unwrap();
    assert_eq!(fee, 5_000);
    assert_eq!(payout, 95_000);
}

#[test]
fn compute_fee_correct_for_250_bps() {
    let (fee, payout) = invariants::compute_fee(80_000, 250).unwrap();
    assert_eq!(fee, 2_000);
    assert_eq!(payout, 78_000);
}

#[test]
fn compute_fee_correct_for_zero_bps() {
    let (fee, payout) = invariants::compute_fee(50_000, 0).unwrap();
    assert_eq!(fee, 0);
    assert_eq!(payout, 50_000);
}

#[test]
fn compute_fee_handles_overflow() {
    let result = invariants::compute_fee(i128::MAX, 10_000);
    assert!(result.is_err());
}

#[test]
fn invariant_error_maps_to_escrow_error() {
    let err: EscrowError = invariants::InvariantError::BalanceMismatch.into();
    assert_eq!(err, EscrowError::BalanceMismatch);

    let err: EscrowError = invariants::InvariantError::InvalidStateTransition.into();
    assert_eq!(err, EscrowError::InvalidStateTransition);

    let err: EscrowError = invariants::InvariantError::EscrowAmountMismatch.into();
    assert_eq!(err, EscrowError::CrossContractConsistencyFailed);
}
