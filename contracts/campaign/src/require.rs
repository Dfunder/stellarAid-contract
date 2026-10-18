//! Input guards for the campaign contract, one per [`Error`] variant
//! (issues #759, #760, #761).
//!
//! # Two-layer design
//!
//! Every guard comes in two halves:
//!
//! * `check_*` — a **pure** function returning `Result<(), Error>`. It never
//!   touches the host, so it is exhaustively unit-testable and can be reused
//!   from off-chain tooling.
//! * [`require!`] — a thin macro that turns an `Err` into
//!   `panic_with_error!(&env, e)`, giving the contract a typed contract error
//!   at the call site.
//!
//! ```ignore
//! use crate::require;
//! require!(env, valid_goal, goal);
//! ```
//!
//! # Return early, before effects
//!
//! Guards are always called before any storage write or external call. A guard
//! that runs after a write would leave the state mutated even though the
//! transaction aborts, and would make the panic harder to attribute.

// The module is a complete, closed taxonomy: it defines one guard per `Error`
// variant so that the enum and its enforcement points cannot drift apart, and
// so a future entry point can reach for an existing guard instead of inventing
// a new panic string. Not every guard has a call site yet (the dispute,
// refund and multi-asset lifecycles are landing separately), and this crate is
// built as a `cdylib`, so rustc would otherwise report the unclaimed ones.
#![allow(dead_code)]

use crate::errors::Error;
use crate::math;
use shared::types::CampaignStatus;
use soroban_sdk::{panic_with_error, Env, String};

/// Turns a `check_*` result into a typed contract error panic.
///
/// ```ignore
/// require!(env, valid_goal, goal);
/// require!(env, campaign_open, campaign.status);
/// ```
#[macro_export]
macro_rules! require {
    ($env:expr, $check:ident $(, $arg:expr)*) => {
        if let ::core::result::Result::Err(e) = $crate::require::$check($($arg),*) {
            $crate::errors::raise(&$env, e);
        }
    };
}

pub use self::check_not_initialized as not_initialized;
pub use self::check_initialized as initialized;
pub use self::check_authorized as authorized;
pub use self::check_not_frozen as not_frozen;
pub use self::check_not_locked as not_locked;
pub use self::check_campaign_open as campaign_open;
pub use self::check_valid_goal as valid_goal;
pub use self::check_valid_end_time as valid_end_time;
pub use self::check_asset_accepted as asset_accepted;
pub use self::check_valid_asset_code as valid_asset_code;
pub use self::check_donation_minimum as donation_minimum;
pub use self::check_contract_balance as contract_balance;
pub use self::check_no_funds_held as no_funds_held;
pub use self::check_refund_window_open as refund_window_open;
pub use self::check_no_open_dispute as no_open_dispute;
pub use self::check_active_dispute as active_dispute;
pub use self::check_dispute_cooldown as dispute_cooldown;
pub use self::check_withdrawal_window as withdrawal_window;
pub use self::check_pending_withdrawal as pending_withdrawal;
pub use self::check_accumulate as accumulate;

// ── Lifecycle ──────────────────────────────────────────────────────────────────

/// `initialize` refuses to run twice.
#[inline]
pub fn check_not_initialized(already_initialized: bool) -> Result<(), Error> {
    if already_initialized {
        Err(Error::AlreadyInitialized)
    } else {
        Ok(())
    }
}

/// State-changing entry points refuse to run before `initialize`.
#[inline]
pub fn check_initialized(initialized: bool) -> Result<(), Error> {
    if initialized {
        Ok(())
    } else {
        Err(Error::NotInitialized)
    }
}

/// The caller authenticated as the address the action is scoped to.
#[inline]
pub fn check_authorized(is_authorized: bool) -> Result<(), Error> {
    if is_authorized {
        Ok(())
    } else {
        Err(Error::Unauthorized)
    }
}

/// A frozen contract accepts no state change.
#[inline]
pub fn check_not_frozen(frozen: bool) -> Result<(), Error> {
    if frozen {
        Err(Error::ContractFrozen)
    } else {
        Ok(())
    }
}

/// A re-entrant call is rejected before it can touch any state.
#[inline]
pub fn check_not_locked(locked: bool) -> Result<(), Error> {
    if locked {
        Err(Error::Reentrant)
    } else {
        Ok(())
    }
}

// ── Campaign configuration ─────────────────────────────────────────────────────

/// A closed campaign accepts no further writes; a live one must be `Active`.
#[inline]
pub fn check_campaign_open(status: CampaignStatus) -> Result<(), Error> {
    match status {
        CampaignStatus::Completed | CampaignStatus::Cancelled => Err(Error::CampaignEnded),
        CampaignStatus::Active => Ok(()),
        _ => Err(Error::CampaignNotActive),
    }
}

/// A campaign goal must be strictly positive.
#[inline]
pub fn check_valid_goal(goal: i128) -> Result<(), Error> {
    if goal <= 0 {
        Err(Error::InvalidGoalAmount)
    } else {
        Ok(())
    }
}

/// An end time must be in the future and within `max_offset` seconds of `now`.
///
/// The 2-year horizon bounds how far ahead a creator can commit donors, which
/// is what keeps a campaign from being pushed beyond any realistic indexer's
/// retention window.
#[inline]
pub fn check_valid_end_time(now: u64, end_time: u64, max_offset: u64) -> Result<(), Error> {
    if end_time <= now {
        return Err(Error::InvalidEndTime);
    }
    match now.checked_add(max_offset) {
        Some(ceiling) if end_time <= ceiling => Ok(()),
        _ => Err(Error::InvalidEndTime),
    }
}

// ── Assets and amounts ─────────────────────────────────────────────────────────

/// The asset backing a donation is on the campaign's accepted list.
#[inline]
pub fn check_asset_accepted(accepted: bool) -> Result<(), Error> {
    if accepted {
        Ok(())
    } else {
        Err(Error::AssetNotAccepted)
    }
}

/// An asset code is at most `max_len` bytes and non-empty.
#[inline]
pub fn check_valid_asset_code(code: &String, max_len: u32) -> Result<(), Error> {
    if code.is_empty() || code.len() > max_len {
        Err(Error::InvalidAssets)
    } else {
        Ok(())
    }
}

/// A donation is large enough to be worth recording.
#[inline]
pub fn check_donation_minimum(amount: i128, minimum: i128) -> Result<(), Error> {
    if amount < minimum {
        Err(Error::DonationTooSmall)
    } else {
        Ok(())
    }
}

/// The contract's recorded balance covers a payout.
///
/// Checked **before** the transfer so a doomed payout never reaches the token
/// contract, and in the same invocation as the transfer so the two cannot be
/// separated by an interleaved call.
#[inline]
pub fn check_contract_balance(balance: i128, required: i128) -> Result<(), Error> {
    if balance < required {
        Err(Error::InsufficientContractBalance)
    } else {
        Ok(())
    }
}

// ── Cancellation and refunds ───────────────────────────────────────────────────

/// A campaign holding raised funds must be resolved, not cancelled.
#[inline]
pub fn check_no_funds_held(raised: i128) -> Result<(), Error> {
    if raised > 0 {
        Err(Error::CannotCancelWithFunds)
    } else {
        Ok(())
    }
}

/// Refunds are only accepted inside the campaign's refund window.
#[inline]
pub fn check_refund_window_open(open: bool) -> Result<(), Error> {
    if open {
        Ok(())
    } else {
        Err(Error::RefundWindowClosed)
    }
}

// ── Disputes ───────────────────────────────────────────────────────────────────

/// A dispute cannot be raised again while one is already open.
#[inline]
pub fn check_no_open_dispute(dispute_open: bool) -> Result<(), Error> {
    if dispute_open {
        Err(Error::DisputeAlreadyOpen)
    } else {
        Ok(())
    }
}

/// Resolution requires an open dispute.
#[inline]
pub fn check_active_dispute(dispute_open: bool) -> Result<(), Error> {
    if dispute_open {
        Ok(())
    } else {
        Err(Error::NoActiveDispute)
    }
}

/// Two disputes on the same campaign must be separated by a cooldown.
#[inline]
pub fn check_dispute_cooldown(elapsed: u64, cooldown: u64) -> Result<(), Error> {
    if elapsed < cooldown {
        Err(Error::DisputeCooldownActive)
    } else {
        Ok(())
    }
}

// ── Withdrawal request lifecycle ───────────────────────────────────────────────

/// Funds may only be drawn once the requested notice window has elapsed.
#[inline]
pub fn check_withdrawal_window(now: u64, available_at: u64) -> Result<(), Error> {
    if now < available_at {
        Err(Error::WithdrawalWindowNotElapsed)
    } else {
        Ok(())
    }
}

/// Cancelling or finalizing requires a pending request to exist.
#[inline]
pub fn check_pending_withdrawal(pending: bool) -> Result<(), Error> {
    if pending {
        Ok(())
    } else {
        Err(Error::NoPendingWithdrawal)
    }
}

// ── Arithmetic ─────────────────────────────────────────────────────────────────

/// Guards a caller-supplied total against a silent wrap, deferring to
/// [`math::add`] for the actual checked addition.
///
/// This exists so `Error::Overflow` has the same shape as every other guard and
/// can be exercised by the negative-path suite in `negative_tests`.
#[inline]
pub fn check_accumulate(env: &Env, running_total: i128, delta: i128) -> Result<(), Error> {
    math::add(env, running_total, delta);
    Ok(())
}

/// Panics with `error` outside the macro, for call sites that already hold a
/// `Result` and want to propagate it as a contract error.
#[inline]
pub fn unwrap_or_raise<T>(env: &Env, result: Result<T, Error>) -> T {
    match result {
        Ok(v) => v,
        Err(e) => panic_with_error!(env, e),
    }
}
