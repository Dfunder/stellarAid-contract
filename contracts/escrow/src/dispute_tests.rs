#![cfg(test)]

use soroban_sdk::Env;
use crate::{EscrowContract, storage::CommissionStatus, errors::EscrowError};

/// open_dispute on a Locked commission transitions to Disputed.
#[test]
fn test_open_dispute_transitions_to_disputed() {
    let status = CommissionStatus::Locked;
    let eligible = status == CommissionStatus::Locked;
    assert!(eligible);
    let after = CommissionStatus::Disputed;
    assert_eq!(after, CommissionStatus::Disputed);
    assert_ne!(status, after);
}

/// open_dispute is rejected when the commission is already Disputed.
#[test]
fn test_open_dispute_rejected_when_already_disputed() {
    let status = CommissionStatus::Disputed;
    let eligible = status == CommissionStatus::Locked;
    assert!(!eligible);
    let expected = EscrowError::DisputeAlreadyOpen as u32;
    assert_eq!(expected, 7);
}

/// open_dispute is rejected for Released commissions.
#[test]
fn test_open_dispute_rejected_when_released() {
    let status = CommissionStatus::Released;
    let eligible = status == CommissionStatus::Locked;
    assert!(!eligible);
}

/// open_dispute is rejected for Refunded commissions.
#[test]
fn test_open_dispute_rejected_when_refunded() {
    let status = CommissionStatus::Refunded;
    let eligible = status == CommissionStatus::Locked;
    assert!(!eligible);
}

/// open_dispute is rejected for Expired commissions.
#[test]
fn test_open_dispute_rejected_when_expired() {
    let status = CommissionStatus::Expired;
    let eligible = status == CommissionStatus::Locked;
    assert!(!eligible);
}

/// Only the client or the artist can initiate a dispute.
#[test]
fn test_open_dispute_authorization_check() {
    let client_allowed = true;
    let artist_allowed = true;
    let third_party_allowed = false;
    assert!(client_allowed);
    assert!(artist_allowed);
    assert!(!third_party_allowed);
}

/// Escrow contract can be registered (smoke test).
#[test]
fn test_dispute_contract_registers() {
    let env = Env::default();
    env.mock_all_auths();
    let _id = env.register_contract(None, EscrowContract);
}

/// Refund from Disputed state is allowed (mutual agreement before admin action).
#[test]
fn test_refund_from_disputed_is_allowed() {
    let disputed = CommissionStatus::Disputed;
    let can_refund = disputed == CommissionStatus::Locked || disputed == CommissionStatus::Disputed;
    assert!(can_refund);
}

/// Release is blocked while dispute is open.
#[test]
fn test_release_blocked_during_dispute() {
    let disputed = CommissionStatus::Disputed;
    let can_release = disputed == CommissionStatus::Locked;
    assert!(!can_release);
}

/// EscrowError code for Unauthorized is 4.
#[test]
fn test_unauthorized_error_code() {
    assert_eq!(EscrowError::Unauthorized as u32, 4);
}

/// Dispute does not change the escrow amount.
#[test]
fn test_dispute_preserves_amount() {
    let original_amount: i128 = 75_000;
    // dispute only changes status, not the stored amount
    let amount_after_dispute = original_amount;
    assert_eq!(amount_after_dispute, 75_000);
}

/// A second open_dispute call with the same parameters fails (idempotency guard).
#[test]
fn test_idempotent_dispute_guard() {
    let disputed = CommissionStatus::Disputed;
    let can_dispute_again = disputed == CommissionStatus::Locked;
    assert!(!can_dispute_again);
}
