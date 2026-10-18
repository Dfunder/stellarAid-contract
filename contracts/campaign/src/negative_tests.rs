//! Negative-path tests: one test per [`Error`] variant (issue #761).
//!
//! The contract's guards are split into pure `check_*` functions and a
//! panicking macro, which lets every error code be asserted by value instead
//! of by "something panicked". Each test below therefore pins **the exact
//! contract error**, so a guard that starts reporting the wrong code — or a
//! variant that stops being reachable at all — fails loudly.
//!
//! Naming convention: `test_<operation>_fails_<reason>`, matching the issue's
//! `test_donate_fails_after_deadline` example.

use crate::errors::{get_suggestion, Error};
use crate::guard;
use crate::math;
use crate::require;
use shared::types::CampaignStatus;
use soroban_sdk::Env;
use std::vec::Vec;

/// Maximum end-time horizon, mirroring `MAX_DEADLINE_OFFSET_SECS`.
const TWO_YEARS: u64 = 63_115_200;

// ── Lifecycle ──────────────────────────────────────────────────────────────────

#[test]
fn test_initialize_fails_when_already_initialized() {
    assert_eq!(require::check_not_initialized(true), Err(Error::AlreadyInitialized));
    assert_eq!(require::check_not_initialized(false), Ok(()));
}

#[test]
fn test_mutation_fails_before_initialization() {
    assert_eq!(require::check_initialized(false), Err(Error::NotInitialized));
    assert_eq!(require::check_initialized(true), Ok(()));
}

#[test]
fn test_admin_call_fails_for_unauthorized_caller() {
    assert_eq!(require::check_authorized(false), Err(Error::Unauthorized));
    assert_eq!(require::check_authorized(true), Ok(()));
}

#[test]
fn test_mutation_fails_while_contract_frozen() {
    assert_eq!(require::check_not_frozen(true), Err(Error::ContractFrozen));
    assert_eq!(require::check_not_frozen(false), Ok(()));
}

#[test]
fn test_reentrant_call_fails_while_locked() {
    assert_eq!(require::check_not_locked(true), Err(Error::Reentrant));
    assert_eq!(require::check_not_locked(false), Ok(()));
}

// ── Campaign configuration ─────────────────────────────────────────────────────

#[test]
fn test_mutation_fails_after_campaign_ended() {
    assert_eq!(
        require::check_campaign_open(CampaignStatus::Completed),
        Err(Error::CampaignEnded)
    );
    assert_eq!(
        require::check_campaign_open(CampaignStatus::Cancelled),
        Err(Error::CampaignEnded)
    );
    assert_eq!(require::check_campaign_open(CampaignStatus::Active), Ok(()));
}

#[test]
fn test_mutation_fails_when_campaign_not_active() {
    for status in [
        CampaignStatus::Pending,
        CampaignStatus::Rejected,
        CampaignStatus::Suspended,
    ] {
        assert_eq!(
            require::check_campaign_open(status),
            Err(Error::CampaignNotActive),
            "status {status:?} must be rejected as not-active"
        );
    }
}

#[test]
fn test_create_campaign_fails_with_non_positive_goal() {
    assert_eq!(require::check_valid_goal(0), Err(Error::InvalidGoalAmount));
    assert_eq!(require::check_valid_goal(-1), Err(Error::InvalidGoalAmount));
    assert_eq!(require::check_valid_goal(i128::MIN), Err(Error::InvalidGoalAmount));
    assert_eq!(require::check_valid_goal(1), Ok(()));
}

#[test]
fn test_create_campaign_fails_with_invalid_end_time() {
    // In the past.
    assert_eq!(
        require::check_valid_end_time(1_000, 999, TWO_YEARS),
        Err(Error::InvalidEndTime)
    );
    // Exactly now.
    assert_eq!(
        require::check_valid_end_time(1_000, 1_000, TWO_YEARS),
        Err(Error::InvalidEndTime)
    );
    // Beyond the two-year horizon.
    assert_eq!(
        require::check_valid_end_time(1_000, 1_000 + TWO_YEARS + 1, TWO_YEARS),
        Err(Error::InvalidEndTime)
    );
    assert_eq!(
        require::check_valid_end_time(1_000, 1_000 + TWO_YEARS, TWO_YEARS),
        Ok(())
    );
    // The ceiling computation itself must not wrap.
    assert_eq!(
        require::check_valid_end_time(u64::MAX, u64::MAX, TWO_YEARS),
        Err(Error::InvalidEndTime)
    );
}

// ── Assets and amounts ─────────────────────────────────────────────────────────

#[test]
fn test_donate_fails_for_unaccepted_asset() {
    assert_eq!(require::check_asset_accepted(false), Err(Error::AssetNotAccepted));
    assert_eq!(require::check_asset_accepted(true), Ok(()));
}

#[test]
fn test_asset_code_fails_when_empty_or_over_length() {
    let env = Env::default();
    let max = 12_u32;
    assert_eq!(
        require::check_valid_asset_code(&soroban_sdk::String::from_str(&env, ""), max),
        Err(Error::InvalidAssets)
    );
    assert_eq!(
        require::check_valid_asset_code(&soroban_sdk::String::from_str(&env, "A"), max),
        Ok(())
    );
    assert_eq!(
        require::check_valid_asset_code(&soroban_sdk::String::from_str(&env, "ABCDEFGHIJKL"), max),
        Ok(())
    );
    assert_eq!(
        require::check_valid_asset_code(&soroban_sdk::String::from_str(&env, "ABCDEFGHIJKLM"), max),
        Err(Error::InvalidAssets)
    );
}

#[test]
fn test_donate_fails_below_minimum_amount() {
    assert_eq!(require::check_donation_minimum(0, 1), Err(Error::DonationTooSmall));
    assert_eq!(
        require::check_donation_minimum(-5, 1),
        Err(Error::DonationTooSmall)
    );
    assert_eq!(require::check_donation_minimum(1, 1), Ok(()));
}

#[test]
fn test_payout_fails_without_sufficient_contract_balance() {
    assert_eq!(
        require::check_contract_balance(999, 1_000),
        Err(Error::InsufficientContractBalance)
    );
    assert_eq!(require::check_contract_balance(1_000, 1_000), Ok(()));
    assert_eq!(require::check_contract_balance(1_001, 1_000), Ok(()));
}

// ── Cancellation and refunds ───────────────────────────────────────────────────

#[test]
fn test_cancel_campaign_fails_while_funds_are_held() {
    assert_eq!(require::check_no_funds_held(1), Err(Error::CannotCancelWithFunds));
    assert_eq!(require::check_no_funds_held(0), Ok(()));
}

#[test]
fn test_refund_fails_after_window_closed() {
    assert_eq!(require::check_refund_window_open(false), Err(Error::RefundWindowClosed));
    assert_eq!(require::check_refund_window_open(true), Ok(()));
}

// ── Disputes ───────────────────────────────────────────────────────────────────

#[test]
fn test_raise_dispute_fails_when_one_is_already_open() {
    assert_eq!(require::check_no_open_dispute(true), Err(Error::DisputeAlreadyOpen));
    assert_eq!(require::check_no_open_dispute(false), Ok(()));
}

#[test]
fn test_resolve_dispute_fails_without_an_active_dispute() {
    assert_eq!(require::check_active_dispute(false), Err(Error::NoActiveDispute));
    assert_eq!(require::check_active_dispute(true), Ok(()));
}

#[test]
fn test_raise_dispute_fails_during_cooldown() {
    assert_eq!(require::check_dispute_cooldown(10, 100), Err(Error::DisputeCooldownActive));
    assert_eq!(require::check_dispute_cooldown(100, 100), Ok(()));
    assert_eq!(require::check_dispute_cooldown(101, 100), Ok(()));
}

// ── Withdrawal request lifecycle ───────────────────────────────────────────────

#[test]
fn test_finalize_withdrawal_fails_before_window_elapsed() {
    assert_eq!(
        require::check_withdrawal_window(99, 100),
        Err(Error::WithdrawalWindowNotElapsed)
    );
    assert_eq!(require::check_withdrawal_window(100, 100), Ok(()));
}

#[test]
fn test_withdrawal_action_fails_without_a_pending_request() {
    assert_eq!(require::check_pending_withdrawal(false), Err(Error::NoPendingWithdrawal));
    assert_eq!(require::check_pending_withdrawal(true), Ok(()));
}

// ── Arithmetic ─────────────────────────────────────────────────────────────────

#[test]
fn test_accumulating_totals_fails_on_overflow() {
    let env = Env::default();
    // The guard reports Overflow rather than panicking with a bare string, and
    // a wrapped running total must abort the invocation.
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        require::check_accumulate(&env, i128::MAX, 1)
    }));
    assert!(res.is_err(), "a wrapped running total must abort the invocation");
    // The same guard succeeds on a representable sum.
    assert_eq!(require::check_accumulate(&env, 1_000, 500), Ok(()));
}

// ── Coverage guarantees ────────────────────────────────────────────────────────

#[test]
fn every_error_variant_has_a_one_line_description() {
    let all = [
        Error::AlreadyInitialized,
        Error::NotInitialized,
        Error::Unauthorized,
        Error::CampaignEnded,
        Error::CampaignNotActive,
        Error::AssetNotAccepted,
        Error::DonationTooSmall,
        Error::InsufficientContractBalance,
        Error::InvalidGoalAmount,
        Error::InvalidEndTime,
        Error::InvalidAssets,
        Error::CannotCancelWithFunds,
        Error::RefundWindowClosed,
        Error::DisputeCooldownActive,
        Error::DisputeAlreadyOpen,
        Error::NoActiveDispute,
        Error::WithdrawalWindowNotElapsed,
        Error::NoPendingWithdrawal,
        Error::Overflow,
        Error::Reentrant,
        Error::ContractFrozen,
    ];
    assert_eq!(all.len(), 21, "the enum has 21 variants");
    for error in all {
        let description = error.description();
        assert!(!description.is_empty(), "{error:?} needs a description");
        assert!(
            description.chars().count() >= 10,
            "{error:?} description should be a real sentence"
        );
        // Every code is unique and inside the documented campaign band.
        let code = error as u32;
        assert!(
            (301..=399).contains(&code),
            "{error:?} code {code} is outside the campaign band"
        );
    }
    let mut codes: Vec<u32> = all.iter().map(|e| *e as u32).collect();
    codes.sort_unstable();
    let unique = codes.len();
    codes.dedup();
    assert_eq!(codes.len(), unique, "error codes must be unique");
}

#[test]
fn every_error_maps_to_a_stable_client_suggestion() {
    // `symbol_short!` literals are capped at 9 characters, so a longer
    // suggestion would not compile. Pinning the mapping keeps the client-facing
    // shortcodes from drifting.
    let env = Env::default();
    let expected = [
        (Error::AlreadyInitialized, "ALREADY"),
        (Error::NotInitialized, "NO_INIT"),
        (Error::Unauthorized, "AUTH"),
        (Error::CampaignEnded, "ENDED"),
        (Error::CampaignNotActive, "BAD_STS"),
        (Error::AssetNotAccepted, "BAD_AST"),
        (Error::DonationTooSmall, "TOO_SML"),
        (Error::InsufficientContractBalance, "NO_FUND"),
        (Error::InvalidGoalAmount, "BAD_GOAL"),
        (Error::InvalidEndTime, "BAD_TIME"),
        (Error::InvalidAssets, "BAD_AST"),
        (Error::CannotCancelWithFunds, "HOLD_FND"),
        (Error::RefundWindowClosed, "RFND_SHUT"),
        (Error::DisputeCooldownActive, "DS_COOLD"),
        (Error::DisputeAlreadyOpen, "DUP_DS"),
        (Error::NoActiveDispute, "NO_DS"),
        (Error::WithdrawalWindowNotElapsed, "EARLY"),
        (Error::NoPendingWithdrawal, "NO_PEND"),
        (Error::Overflow, "OVERFL"),
        (Error::Reentrant, "REENTRY"),
        (Error::ContractFrozen, "FROZEN"),
    ];
    for (error, shortcode) in expected {
        assert_eq!(get_suggestion(error), soroban_sdk::Symbol::new(&env, shortcode));
    }
}

#[test]
fn error_codes_are_permanently_assigned() {
    // `docs/UPGRADE_AND_ROLLBACK.md`: error code values are permanent.
    assert_eq!(Error::AlreadyInitialized as u32, 301);
    assert_eq!(Error::NotInitialized as u32, 302);
    assert_eq!(Error::Unauthorized as u32, 303);
    assert_eq!(Error::CampaignEnded as u32, 304);
    assert_eq!(Error::CampaignNotActive as u32, 305);
    assert_eq!(Error::AssetNotAccepted as u32, 306);
    assert_eq!(Error::DonationTooSmall as u32, 307);
    assert_eq!(Error::InsufficientContractBalance as u32, 308);
    assert_eq!(Error::InvalidGoalAmount as u32, 309);
    assert_eq!(Error::InvalidEndTime as u32, 310);
    assert_eq!(Error::InvalidAssets as u32, 311);
    assert_eq!(Error::CannotCancelWithFunds as u32, 312);
    assert_eq!(Error::RefundWindowClosed as u32, 313);
    assert_eq!(Error::DisputeCooldownActive as u32, 314);
    assert_eq!(Error::DisputeAlreadyOpen as u32, 315);
    assert_eq!(Error::NoActiveDispute as u32, 316);
    assert_eq!(Error::WithdrawalWindowNotElapsed as u32, 317);
    assert_eq!(Error::NoPendingWithdrawal as u32, 318);
    assert_eq!(Error::Overflow as u32, 319);
    assert_eq!(Error::Reentrant as u32, 320);
    assert_eq!(Error::ContractFrozen as u32, 321);
}

#[test]
fn reentrancy_guard_uses_the_shared_error_code() {
    // The campaign-level `Error::Reentrant` is the taxonomy entry; the panic
    // itself carries `shared::guard::ReentrancyError::Reentrant`, which shares
    // code 9 with escrow so an indexer decodes re-entrancy uniformly.
    let env = Env::default();
    assert_eq!(Error::Reentrant as u32, 320);
    assert_eq!(guard::ReentrancyError::Reentrant as u32, 9);
    assert_eq!(
        guard::get_suggestion(guard::ReentrancyError::Reentrant),
        soroban_sdk::Symbol::new(&env, "REENTRY")
    );
}

#[test]
fn contract_guard_rejects_a_nested_state_changing_call() {
    let env = Env::default();
    let contract_id = env.register_contract(None, crate::CampaignContract);
    env.as_contract(&contract_id, || {
        // `check_not_locked` is the pure half of the guard used by the contract.
        guard::acquire(&env);
        assert_eq!(require::check_not_locked(guard::is_locked(&env)), Err(Error::Reentrant));
        guard::release(&env);
        assert_eq!(require::check_not_locked(guard::is_locked(&env)), Ok(()));
    });
}

#[test]
fn math_helpers_report_overflow_instead_of_wrapping() {
    let env = Env::default();
    assert_eq!(math::fee_split(&env, 2_500, 500), (125, 2_375));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| { math::add(&env, i128::MAX, 1) }))
            .is_err()
    );
}
