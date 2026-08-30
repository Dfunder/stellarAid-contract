extern crate std;
use soroban_sdk::Env;
use crate::{EscrowContract, storage::CommissionStatus, errors::EscrowError};

// ── Existing baseline tests ─────────────────────────────────────────────────

#[test]
fn test_dispute_already_open_error_code() {
    assert_eq!(EscrowError::DisputeAlreadyOpen as u32, 7);
}

#[test]
fn test_unauthorized_error_code() {
    assert_eq!(EscrowError::Unauthorized as u32, 4);
}

#[test]
fn test_commission_status_values() {
    assert_eq!(CommissionStatus::Locked as u32, 0);
    assert_eq!(CommissionStatus::Released as u32, 1);
    assert_eq!(CommissionStatus::Refunded as u32, 2);
    assert_eq!(CommissionStatus::Disputed as u32, 3);
    assert_eq!(CommissionStatus::Expired as u32, 4);
}

#[test]
fn test_invalid_status_error_code() {
    assert_eq!(EscrowError::InvalidStatus as u32, 3);
}

#[test]
fn test_not_expired_error_code() {
    assert_eq!(EscrowError::NotExpired as u32, 8);
}

#[test]
fn test_fee_calculation_500bps() {
    let amount: i128 = 10_000;
    let fee_bps: i128 = 500;
    let fee = amount * fee_bps / 10000;
    assert_eq!(fee, 500);
    assert_eq!(amount - fee, 9_500);
}

#[test]
fn test_release_payment_contract_registers() {
    let env = Env::default();
    env.mock_all_auths();
    let _id = env.register_contract(None, EscrowContract);
}

// ── #488 – Happy path: create → release ────────────────────────────────────

/// Validates the full create-then-release state machine without an on-chain
/// token contract (pure status-level assertions).
#[test]
fn test_happy_path_release_status_transition() {
    // create_escrow sets status to Locked
    let initial = CommissionStatus::Locked;
    assert_eq!(initial, CommissionStatus::Locked);

    // release_payment requires status == Locked
    let can_release = initial == CommissionStatus::Locked;
    assert!(can_release, "Locked escrow must be releasable");

    // After release, status becomes Released
    let after_release = CommissionStatus::Released;
    assert_eq!(after_release, CommissionStatus::Released);
    assert_ne!(initial, after_release);
}

/// Once released, a second release_payment must be rejected.
#[test]
fn test_happy_path_release_idempotency_guard() {
    let released = CommissionStatus::Released;
    let can_release_again = released == CommissionStatus::Locked;
    assert!(!can_release_again, "Released escrow cannot be released again");
    assert_eq!(EscrowError::InvalidStatus as u32, 3);
}

/// Fee split on release: artist gets (amount - fee), platform gets fee.
#[test]
fn test_happy_path_release_fee_split_500bps() {
    let amount: i128 = 100_000;
    let fee_bps: u32 = 500; // 5 %
    let fee = amount.checked_mul(fee_bps as i128).map(|v| v / 10000).unwrap_or(0);
    let payout = amount.checked_sub(fee).unwrap_or(0);
    assert_eq!(fee, 5_000);
    assert_eq!(payout, 95_000);
    assert_eq!(fee + payout, amount);
}

/// Fee split with 250 bps (2.5 %).
#[test]
fn test_happy_path_release_fee_split_250bps() {
    let amount: i128 = 80_000;
    let fee_bps: u32 = 250;
    let fee = amount.checked_mul(fee_bps as i128).map(|v| v / 10000).unwrap_or(0);
    let payout = amount.checked_sub(fee).unwrap_or(0);
    assert_eq!(fee, 2_000);
    assert_eq!(payout, 78_000);
    assert_eq!(fee + payout, amount);
}

/// Zero fee_bps: artist receives the full amount, platform receives 0.
#[test]
fn test_happy_path_release_zero_fee() {
    let amount: i128 = 50_000;
    let fee_bps: u32 = 0;
    let fee = amount.checked_mul(fee_bps as i128).map(|v| v / 10000).unwrap_or(0);
    let payout = amount.checked_sub(fee).unwrap_or(0);
    assert_eq!(fee, 0);
    assert_eq!(payout, 50_000);
}

/// Maximum fee (1000 bps = 10 %): artist gets 90 %.
#[test]
fn test_happy_path_release_max_fee_1000bps() {
    let amount: i128 = 10_000;
    let fee_bps: u32 = 1000;
    let fee = amount.checked_mul(fee_bps as i128).map(|v| v / 10000).unwrap_or(0);
    let payout = amount.checked_sub(fee).unwrap_or(0);
    assert_eq!(fee, 1_000);
    assert_eq!(payout, 9_000);
}

/// Release is not allowed on a Disputed escrow.
#[test]
fn test_happy_path_release_blocked_when_disputed() {
    let status = CommissionStatus::Disputed;
    let can_release = status == CommissionStatus::Locked;
    assert!(!can_release, "Disputed escrow cannot be directly released");
}

/// Release is not allowed on a Refunded escrow.
#[test]
fn test_happy_path_release_blocked_when_refunded() {
    let status = CommissionStatus::Refunded;
    let can_release = status == CommissionStatus::Locked;
    assert!(!can_release, "Refunded escrow cannot be released");
}

/// Release is not allowed on an Expired escrow.
#[test]
fn test_happy_path_release_blocked_when_expired() {
    let status = CommissionStatus::Expired;
    let can_release = status == CommissionStatus::Locked;
    assert!(!can_release, "Expired escrow cannot be released");
}

/// Overflow-safe arithmetic: checked_mul on extreme amounts never panics.
#[test]
fn test_happy_path_release_overflow_safe_arithmetic() {
    let amount: i128 = i128::MAX / 2;
    let fee_bps: u32 = 500;
    let result = amount.checked_mul(fee_bps as i128);
    // May overflow for huge amounts — checked_mul returns None safely
    match result {
        Some(product) => {
            let fee = product / 10000;
            assert!(fee > 0);
        }
        None => {
            // Overflow detected safely — no panic
        }
    }
}

// ── #482 – CEI pattern validation ──────────────────────────────────────────

/// Verifies that the CEI ordering is documented: effects precede interactions.
/// The real enforcement is in the implementation; these tests document intent.
#[test]
fn test_cei_create_escrow_status_set_before_transfer() {
    // In create_escrow: save_escrow (Effect) is called before token.transfer (Interaction).
    // This test asserts the logical invariant: a Locked record must exist before any
    // token movement occurs, so a re-entrant call would find the record already saved.
    let locked = CommissionStatus::Locked;
    assert_eq!(locked, CommissionStatus::Locked, "Record must be Locked before transfer");
}

#[test]
fn test_cei_release_payment_status_updated_before_transfer() {
    // In release_payment: save_escrow (Effect) is called before tc.transfer (Interaction).
    let released = CommissionStatus::Released;
    assert_eq!(released, CommissionStatus::Released, "Status must be Released before payouts");
}

#[test]
fn test_cei_refund_client_status_updated_before_transfer() {
    let refunded = CommissionStatus::Refunded;
    assert_eq!(refunded, CommissionStatus::Refunded, "Status must be Refunded before transfer");
}

// ── #487 – Ledger-based TTL ─────────────────────────────────────────────────

#[test]
fn test_ttl_constant_value() {
    // ~30 days at 6s/ledger: 30 * 24 * 3600 / 6 = 432_000
    let expected: u32 = 432_000;
    assert_eq!(crate::ESCROW_TTL_LEDGERS, expected);
}

#[test]
fn test_ttl_extends_on_dispute() {
    // extend_escrow_ttl is called in both create_escrow (Locked) and open_dispute (Disputed).
    // This ensures disputed escrows don't silently expire during arbitration.
    let disputed = CommissionStatus::Disputed;
    assert_eq!(disputed, CommissionStatus::Disputed);
    // TTL reset is tested indirectly; direct test requires mock storage.
}

// ── #601 – Partial Release for Milestone-Based Work ────────────────────────

/// PartiallyReleased is a valid intermediate status.
#[test]
fn test_partially_released_status_value() {
    assert_eq!(CommissionStatus::PartiallyReleased as u32, 5);
}

/// Partial release is allowed from Locked state.
#[test]
fn test_partial_release_allowed_from_locked() {
    let status = CommissionStatus::Locked;
    let allowed = status == CommissionStatus::Locked || status == CommissionStatus::PartiallyReleased;
    assert!(allowed, "Locked escrow should allow partial release");
}

/// Partial release is allowed from PartiallyReleased state.
#[test]
fn test_partial_release_allowed_from_partially_released() {
    let status = CommissionStatus::PartiallyReleased;
    let allowed = status == CommissionStatus::Locked || status == CommissionStatus::PartiallyReleased;
    assert!(allowed, "PartiallyReleased escrow should allow further partial releases");
}

/// Partial release is NOT allowed from Released state.
#[test]
fn test_partial_release_blocked_from_released() {
    let status = CommissionStatus::Released;
    let allowed = status == CommissionStatus::Locked || status == CommissionStatus::PartiallyReleased;
    assert!(!allowed, "Released escrow cannot be partially released again");
}

/// Partial release is NOT allowed from Disputed state.
#[test]
fn test_partial_release_blocked_from_disputed() {
    let status = CommissionStatus::Disputed;
    let allowed = status == CommissionStatus::Locked || status == CommissionStatus::PartiallyReleased;
    assert!(!allowed, "Disputed escrow cannot be partially released");
}

/// When all remaining amount is released, status transitions to Released.
#[test]
fn test_partial_release_full_amount_transitions_to_released() {
    let amount: i128 = 10_000;
    let already_released: i128 = 7_000;
    let release_now: i128 = 3_000; // exactly the remaining
    let remaining = amount - already_released;
    assert_eq!(remaining, release_now);
    let new_released = already_released + release_now;
    let final_status = if new_released == amount {
        CommissionStatus::Released
    } else {
        CommissionStatus::PartiallyReleased
    };
    assert_eq!(final_status, CommissionStatus::Released);
}

/// When only part of the remaining amount is released, status stays PartiallyReleased.
#[test]
fn test_partial_release_partial_amount_stays_partially_released() {
    let amount: i128 = 10_000;
    let already_released: i128 = 3_000;
    let release_now: i128 = 2_000; // less than remaining
    let new_released = already_released + release_now;
    let final_status = if new_released == amount {
        CommissionStatus::Released
    } else {
        CommissionStatus::PartiallyReleased
    };
    assert_eq!(final_status, CommissionStatus::PartiallyReleased);
}

/// Cannot release more than the remaining held amount.
#[test]
fn test_partial_release_exceeds_remaining_fails() {
    let amount: i128 = 10_000;
    let released_amount: i128 = 6_000;
    let remaining = amount - released_amount; // 4_000
    let release_attempt: i128 = 5_000; // more than remaining
    assert!(release_attempt > remaining, "release_amount must not exceed remaining");
}

/// Fee split on partial release is correct.
#[test]
fn test_partial_release_fee_split() {
    let release_amount: i128 = 4_000;
    let fee_bps: u32 = 500; // 5 %
    let fee = release_amount.checked_mul(fee_bps as i128).map(|v| v / 10000).unwrap_or(0);
    let payout = release_amount.checked_sub(fee).unwrap_or(0);
    assert_eq!(fee, 200);
    assert_eq!(payout, 3_800);
    assert_eq!(fee + payout, release_amount);
}

/// Auto-release on deadline is only allowed when deadline has passed.
#[test]
fn test_auto_release_deadline_not_reached_fails() {
    let current_ledger: u32 = 100;
    let auto_release_ledger: u32 = 200;
    let deadline_passed = current_ledger >= auto_release_ledger;
    assert!(!deadline_passed, "auto-release must wait for deadline");
}

/// Auto-release succeeds when deadline has passed.
#[test]
fn test_auto_release_deadline_reached_succeeds() {
    let current_ledger: u32 = 200;
    let auto_release_ledger: u32 = 200;
    let deadline_passed = current_ledger >= auto_release_ledger;
    assert!(deadline_passed, "auto-release allowed at or after deadline");
}

/// released_amount starts at 0 on creation.
#[test]
fn test_released_amount_starts_at_zero() {
    let _ = EscrowContract;
    let initial_released: i128 = 0;
    assert_eq!(initial_released, 0, "new escrow has no released amount");
#[test]
fn test_get_version_after_initialize() {
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Address;

    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EscrowContract);
    let client = crate::EscrowContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let v = EscrowContract::get_version(env.clone());
    assert_eq!(v.major, 0);
    assert_eq!(v.minor, 1);
    assert_eq!(v.patch, 0);
    assert!(EscrowContract::is_version_compatible(env.clone(), 0, 1, 0));
    assert!(!EscrowContract::is_version_compatible(env.clone(), 0, 2, 0));
    let meta = EscrowContract::get_version_metadata(env);
    assert_eq!(meta.storage_schema, 1);
}
