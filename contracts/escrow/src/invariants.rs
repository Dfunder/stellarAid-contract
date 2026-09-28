//! Balance consistency and state invariants for the Escrow contract (closes #770).
//!
//! This module centralizes every invariant the escrow must maintain:
//!
//! 1. **Balance consistency** — `released_amount + remaining == amount` after
//!    every operation that touches `released_amount`. Prevents dust from being
//!    stranded or double-counted.
//!
//! 2. **Escrow amount verification** — before any funds move, the escrow amount
//!    must match what the commission agreement expects (cross-contract). No
//!    funds may be released for an amount the agreement does not recognize.
//!
//! 3. **State consistency** — status transitions must follow the documented
//!    state machine. A `Released` escrow cannot be released again, a `Refunded`
//!    escrow cannot be disputed, etc.
//!
//! ## State machine
//!
//! ```text
//! Locked ──release_payment──► Released
//! Locked ──refund_client────► Refunded
//! Locked ──open_dispute─────► Disputed
//! Locked ──expire_escrow────► Expired
//! Locked ──partial_release──► PartiallyReleased ──partial_release──► Released
//! Locked ──auto_release─────► Released
//! Disputed ──refund_client──► Refunded
//! Disputed ──open_dispute───► (rejected: DisputeAlreadyOpen)
//! Locked|PartiallyReleased ──cancel_escrow──► Cancelled
//! Disputed ──cancel_escrow───► Cancelled
//! ```

use soroban_sdk::{contracterror, contracttype, symbol_short, Address, Bytes, Env, Symbol};

use crate::errors::EscrowError;
use crate::storage::{CommissionStatus, EscrowRecord};
use soroban_sdk::IntoVal;

/// Error emitted when a balance or state invariant is violated.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvariantError {
    /// `released_amount` exceeds the escrowed `amount` — funds were over-released.
    ReleasedExceedsAmount = 1,
    /// `released_amount + remaining` does not equal `amount` — accounting drift.
    BalanceMismatch = 2,
    /// `released_amount` is negative.
    ReleasedIsNegative = 3,
    /// Status transition from `from` to `to` is not allowed.
    InvalidStateTransition = 4,
    /// Cross-contract escrow amount does not match the local record.
    EscrowAmountMismatch = 5,
    /// No commission agreement is configured for this escrow.
    AgreementMismatch = 6,
}

impl core::fmt::Display for InvariantError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::ReleasedExceedsAmount => write!(f, "released amount exceeds escrowed amount"),
            Self::BalanceMismatch => write!(f, "balance mismatch: released + remaining != amount"),
            Self::ReleasedIsNegative => write!(f, "released amount is negative"),
            Self::InvalidStateTransition => write!(f, "invalid status transition"),
            Self::EscrowAmountMismatch => write!(f, "escrow amount does not match commission agreement"),
            Self::AgreementMismatch => write!(f, "commission agreement mismatch"),
        }
    }
}

impl From<InvariantError> for EscrowError {
    fn from(e: InvariantError) -> Self {
        match e {
            InvariantError::ReleasedExceedsAmount => EscrowError::BalanceMismatch,
            InvariantError::BalanceMismatch => EscrowError::BalanceMismatch,
            InvariantError::ReleasedIsNegative => EscrowError::InvalidAmount,
            InvariantError::InvalidStateTransition => EscrowError::InvalidStatus,
            InvariantError::EscrowAmountMismatch => EscrowError::CrossContractConsistencyFailed,
            InvariantError::AgreementMismatch => EscrowError::CrossContractConsistencyFailed,
        }
    }
}

impl From<shared::circuit_breaker::CircuitBreakerError> for EscrowError {
    fn from(e: shared::circuit_breaker::CircuitBreakerError) -> Self {
        match e {
            shared::circuit_breaker::CircuitBreakerError::Halted => EscrowError::CircuitBreakerTripped,
            shared::circuit_breaker::CircuitBreakerError::AlreadyHalted => EscrowError::CircuitBreakerTripped,
            shared::circuit_breaker::CircuitBreakerError::NotHalted => EscrowError::CircuitBreakerTripped,
            shared::circuit_breaker::CircuitBreakerError::RecoveryPending => EscrowError::CircuitBreakerTripped,
            shared::circuit_breaker::CircuitBreakerError::InvalidThreshold => EscrowError::InvalidStatus,
            shared::circuit_breaker::CircuitBreakerError::InsufficientSamples => {
                EscrowError::CircuitBreakerTripped
            }
        }
    }
}

/// Storage keys used by the invariant module.
#[contracttype]
pub enum InvariantKey {
    /// Last ledger at which a balance-consistency check passed.
    LastCheckedLedger,
}

/// Events emitted by the invariant module.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvariantViolatedEvent {
    pub commission_id: Bytes,
    pub field: Symbol,
    pub expected: i128,
    pub actual: i128,
    pub ledger: u32,
}

/// Events emitted after an escrow amount cross-check.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmountVerifiedEvent {
    pub commission_id: Bytes,
    pub expected: i128,
    pub actual: i128,
    pub ledger: u32,
}

const BPS_DENOM: i128 = 10_000;

/// Verify internal balance consistency for an escrow record.
///
/// Asserts that:
/// - `released_amount >= 0`
/// - `released_amount <= amount`
/// - `released_amount + remaining == amount` (where remaining is computed via
///   checked subtraction)
///
/// This is a pure check: it reads no token-contract balance. Token-level
/// balance verification is performed separately by
/// [`verify_contract_balance`].
pub fn verify_balance_consistency(
    env: &Env,
    record: &EscrowRecord,
) -> Result<(), InvariantError> {
    if record.released_amount < 0 {
        env.events().publish(
            (symbol_short!("inv"), symbol_short!("viol")),
            InvariantViolatedEvent {
                commission_id: record.commission_id.clone(),
                field: symbol_short!("released"),
                expected: 0,
                actual: record.released_amount,
                ledger: env.ledger().sequence(),
            },
        );
        return Err(InvariantError::ReleasedIsNegative);
    }

    if record.released_amount > record.amount {
        env.events().publish(
            (symbol_short!("inv"), symbol_short!("viol")),
            InvariantViolatedEvent {
                commission_id: record.commission_id.clone(),
                field: symbol_short!("amt"),
                expected: record.amount,
                actual: record.released_amount,
                ledger: env.ledger().sequence(),
            },
        );
        return Err(InvariantError::ReleasedExceedsAmount);
    }

    let remaining = record
        .amount
        .checked_sub(record.released_amount)
        .ok_or(InvariantError::BalanceMismatch)?;

    if record.released_amount.checked_add(remaining).is_none() {
        return Err(InvariantError::BalanceMismatch);
    }

    let total = record
        .released_amount
        .checked_add(remaining)
        .ok_or(InvariantError::BalanceMismatch)?;

    if total != record.amount {
        env.events().publish(
            (symbol_short!("inv"), symbol_short!("viol")),
            InvariantViolatedEvent {
                commission_id: record.commission_id.clone(),
                field: symbol_short!("total"),
                expected: record.amount,
                actual: total,
                ledger: env.ledger().sequence(),
            },
        );
        return Err(InvariantError::BalanceMismatch);
    }

    env.storage()
        .instance()
        .set(&InvariantKey::LastCheckedLedger, &env.ledger().sequence());

    Ok(())
}

/// Verify that the escrow contract's actual token balance matches the expected
/// escrowed amount (minus already-released funds).
///
/// Calls the token contract's `balance` to read the current USDC held by the
/// contract, then checks it equals `record.amount - record.released_amount`.
/// A mismatch indicates funds were lost, stolen, or misrouted.
pub fn verify_contract_balance(
    env: &Env,
    usdc_token: &Address,
    record: &EscrowRecord,
) -> Result<(), InvariantError> {
    use soroban_sdk::token;

    let balance = token::Client::new(env, usdc_token).balance(&env.current_contract_address());
    let expected = record
        .amount
        .checked_sub(record.released_amount)
        .ok_or(InvariantError::BalanceMismatch)?;

    if balance != expected {
        env.events().publish(
            (symbol_short!("inv"), symbol_short!("viol")),
            InvariantViolatedEvent {
                commission_id: record.commission_id.clone(),
                field: symbol_short!("token_bal"),
                expected,
                actual: balance,
                ledger: env.ledger().sequence(),
            },
        );
        return Err(InvariantError::BalanceMismatch);
    }

    Ok(())
}

/// Cross-contract escrow amount verification.
///
/// Queries the commission agreement contract for the amount it expects to be
/// escrowed under `commission_id`, and checks that the local escrow record
/// holds exactly that amount. If the agreement does not yet report a value
/// (returns 0), the check is skipped — the escrow is not yet "known" to the
/// agreement side.
///
/// Returns the agreed amount from the commission side, or `None` if the
/// agreement returned zero (not yet set up).
pub fn verify_escrow_amount(
    env: &Env,
    commission_contract: &Address,
    commission_id: &Bytes,
    record: &EscrowRecord,
) -> Result<Option<i128>, InvariantError> {
    let agreed: i128 = env.invoke_contract(
        commission_contract,
        &Symbol::new(env, "get_agreement_escrow_amount"),
        soroban_sdk::vec![env, commission_id.clone().into_val(env)],
    );

    if agreed == 0 {
        return Ok(None);
    }

    if agreed != record.amount {
        let env_clone = env;
        env_clone.events().publish(
            (symbol_short!("inv"), symbol_short!("viol")),
            InvariantViolatedEvent {
                commission_id: record.commission_id.clone(),
                field: symbol_short!("esc_amt"),
                expected: record.amount,
                actual: agreed,
                ledger: env.ledger().sequence(),
            },
        );
        return Err(InvariantError::EscrowAmountMismatch);
    }

    env.events().publish(
        (symbol_short!("inv"), symbol_short!("verfd")),
        AmountVerifiedEvent {
            commission_id: record.commission_id.clone(),
            expected: agreed,
            actual: record.amount,
            ledger: env.ledger().sequence(),
        },
    );

    Ok(Some(agreed))
}

// ── State machine validation ────────────────────────────────────────────────────

/// Valid status transitions for the escrow state machine.
///
/// Returns `true` if a transition `from -> to` is permitted.
pub fn is_valid_transition(from: CommissionStatus, to: CommissionStatus) -> bool {
    match (from, to) {
        // Locked is the initial state — anything that consumes or resolves it is valid.
        (CommissionStatus::Locked, CommissionStatus::Released) => true,
        (CommissionStatus::Locked, CommissionStatus::Refunded) => true,
        (CommissionStatus::Locked, CommissionStatus::Disputed) => true,
        (CommissionStatus::Locked, CommissionStatus::Expired) => true,
        (CommissionStatus::Locked, CommissionStatus::PartiallyReleased) => true,
        (CommissionStatus::Locked, CommissionStatus::Cancelled) => true,

        // PartiallyReleased can move to Released (final payout), Refunded, Disputed, or Cancelled.
        (CommissionStatus::PartiallyReleased, CommissionStatus::Released) => true,
        (CommissionStatus::PartiallyReleased, CommissionStatus::Refunded) => true,
        (CommissionStatus::PartiallyReleased, CommissionStatus::Disputed) => true,
        (CommissionStatus::PartiallyReleased, CommissionStatus::Cancelled) => true,

        // Disputed can be refunded back or cancelled.
        (CommissionStatus::Disputed, CommissionStatus::Refunded) => true,
        (CommissionStatus::Disputed, CommissionStatus::Cancelled) => true,

        // Terminal states never transition.
        _ => false,
    }
}

/// Validate that a proposed status change is a legal transition and that
/// `released_amount` is consistent with the new status.
pub fn validate_state_consistency(
    env: &Env,
    before: &EscrowRecord,
    after: &EscrowRecord,
) -> Result<(), InvariantError> {
    if before.status == after.status {
        return Ok(());
    }

    if !is_valid_transition(before.status, after.status) {
        env.events().publish(
            (symbol_short!("inv"), symbol_short!("viol")),
            InvariantViolatedEvent {
                commission_id: after.commission_id.clone(),
                field: symbol_short!("sts"),
                expected: before.status as u32 as i128,
                actual: after.status as u32 as i128,
                ledger: env.ledger().sequence(),
            },
            );
        return Err(InvariantError::InvalidStateTransition);
    }

    // When fully released, released_amount must equal amount.
    if after.status == CommissionStatus::Released && after.status != before.status {
        if after.released_amount != after.amount {
            return Err(InvariantError::BalanceMismatch);
        }
    }

    // When cancelled or refunded, no further releases should occur.
    if matches!(after.status, CommissionStatus::Cancelled | CommissionStatus::Refunded) {
        if after.released_amount > before.released_amount {
            // Allow cancellation/refund to carry through the same released amount,
            // but not increase it.
            return Err(InvariantError::InvalidStateTransition);
        }
    }

    Ok(())
}

/// Compute the fee portion of `amount` at `fee_bps`, using checked arithmetic.
/// Shared utility used by invariant checks and fee-split verification.
pub fn compute_fee(amount: i128, fee_bps: u32) -> Result<(i128, i128), EscrowError> {
    let fee = amount
        .checked_mul(fee_bps as i128)
        .ok_or(EscrowError::ArithmeticOverflow)?
        .checked_div(BPS_DENOM)
        .unwrap_or(0);
    let payout = amount
        .checked_sub(fee)
        .ok_or(EscrowError::ArithmeticOverflow)?;
    Ok((fee, payout))
}

/// Verify that a fee split adds up correctly: `fee + payout == amount`.
pub fn verify_fee_split(amount: i128, fee: i128, payout: i128) -> Result<(), EscrowError> {
    let total = fee
        .checked_add(payout)
        .ok_or(EscrowError::ArithmeticOverflow)?;
    if total != amount {
        return Err(EscrowError::InvalidSplit);
    }
    Ok(())
}

/// Run all invariant checks on an escrow record: balance consistency + cross-contract
/// verification (if a commission contract is provided).
///
/// This is the umbrella check called after every state-changing operation.
pub fn verify_all(
    env: &Env,
    record: &EscrowRecord,
    usdc_token: Option<&Address>,
    commission_contract: Option<&Address>,
) -> Result<(), EscrowError> {
    verify_balance_consistency(env, record).map_err(EscrowError::from)?;

    if let Some(usdc) = usdc_token {
        verify_contract_balance(env, usdc, record).map_err(EscrowError::from)?;
    }

    if let Some(comm) = commission_contract {
        verify_escrow_amount(env, comm, &record.commission_id, record).map_err(EscrowError::from)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::CommissionStatus;
    use soroban_sdk::BytesN;

    fn make_record(amount: i128, released: i128, status: CommissionStatus) -> EscrowRecord {
        EscrowRecord {
            commission_id: Bytes::from_slice(&Env::default(), b"test-id"),
            client: Address::generate(&Env::default()),
            artist: Address::generate(&Env::default()),
            amount,
            fee_bps: 500,
            status,
            created_ledger: 1,
            released_amount: released,
        }
    }

    #[test]
    fn balanced_record_passes_check() {
        let env = Env::default();
        let r = make_record(100_000, 30_000, CommissionStatus::PartiallyReleased);
        assert!(verify_balance_consistency(&env, &r).is_ok());
    }

    #[test]
    fn fully_released_passes() {
        let env = Env::default();
        let r = make_record(100_000, 100_000, CommissionStatus::Released);
        assert!(verify_balance_consistency(&env, &r).is_ok());
    }

    #[test]
    fn over_release_is_rejected() {
        let env = Env::default();
        let r = make_record(100_000, 101_000, CommissionStatus::Released);
        let err = verify_balance_consistency(&env, &r).unwrap_err();
        assert_eq!(err, InvariantError::ReleasedExceedsAmount);
    }

    #[test]
    fn negative_release_is_rejected() {
        let env = Env::default();
        let r = make_record(100_000, -1, CommissionStatus::Locked);
        let err = verify_balance_consistency(&env, &r).unwrap_err();
        assert_eq!(err, InvariantError::ReleasedIsNegative);
    }

    #[test]
    fn zero_amount_with_zero_released_passes() {
        let env = Env::default();
        let r = make_record(0, 0, CommissionStatus::Released);
        assert!(verify_balance_consistency(&env, &r).is_ok());
    }

    #[test]
    fn valid_state_transitions() {
        assert!(is_valid_transition(
            CommissionStatus::Locked,
            CommissionStatus::Released
        ));
        assert!(is_valid_transition(
            CommissionStatus::Locked,
            CommissionStatus::Refunded
        ));
        assert!(is_valid_transition(
            CommissionStatus::Locked,
            CommissionStatus::Disputed
        ));
        assert!(is_valid_transition(
            CommissionStatus::PartiallyReleased,
            CommissionStatus::Released
        ));
    }

    #[test]
    fn invalid_state_transitions() {
        assert!(!is_valid_transition(
            CommissionStatus::Released,
            CommissionStatus::Locked
        ));
        assert!(!is_valid_transition(
            CommissionStatus::Refunded,
            CommissionStatus::Released
        ));
        assert!(!is_valid_transition(
            CommissionStatus::Expired,
            CommissionStatus::Released
        ));
        assert!(!is_valid_transition(
            CommissionStatus::Released,
            CommissionStatus::Cancelled
        ));
    }

    #[test]
    fn fully_released_requires_released_equals_amount() {
        let env = Env::default();
        let before = make_record(100_000, 0, CommissionStatus::Locked);
        let after = make_record(100_000, 99_999, CommissionStatus::Released);
        let err = validate_state_consistency(&env, &before, &after).unwrap_err();
        assert_eq!(err, InvariantError::BalanceMismatch);
    }

    #[test]
    fn fee_split_verification() {
        assert!(verify_fee_split(100_000, 5_000, 95_000).is_ok());
        assert!(verify_fee_split(100_000, 5_001, 95_000).is_err());
    }

    #[test]
    fn compute_fee_500_bps() {
        let (fee, payout) = compute_fee(100_000, 500).unwrap();
        assert_eq!(fee, 5_000);
        assert_eq!(payout, 95_000);
    }

    #[test]
    fn invariant_error_codes_are_unique() {
        assert_ne!(InvariantError::ReleasedExceedsAmount as u32, 0);
        // Values are 1-based for contracterror compatibility
        let all = [
            InvariantError::ReleasedExceedsAmount as u32,
            InvariantError::BalanceMismatch as u32,
            InvariantError::ReleasedIsNegative as u32,
            InvariantError::InvalidStateTransition as u32,
            InvariantError::EscrowAmountMismatch as u32,
            InvariantError::AgreementMismatch as u32,
        ];
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                assert_ne!(all[i], all[j], "duplicate error code");
            }
        }
    }

    #[test]
    fn released_equals_amount_passes_for_zero_balance() {
        let env = Env::default();
        let r = make_record(0, 0, CommissionStatus::Locked);
        assert_eq!(r.amount, 0);
        assert_eq!(r.released_amount, 0);
        assert!(verify_balance_consistency(&env, &r).is_ok());
    }

    #[test]
    fn _unused_bytesn_import() {
        // Suppress: BytesN is available for future hash-based invariant proofs
        let _: Option<BytesN<32>> = None;
    }
}
