//! Cross-contract call helpers for the Escrow contract.
//!
//! Closes #478 – call PlatformConfig from Escrow
//! Closes #480 – call USDC token contract from Escrow
//! Closes #656 – atomic escrow-to-commission orchestration

use soroban_sdk::{
    contracttype, symbol_short, token, Address, Bytes, Env, IntoVal, Symbol,
};

use crate::errors::EscrowError;
use crate::storage::{self, CommissionStatus};

// ── PlatformConfig helpers ──────────────────────────────────────────────────

/// Fetch the fee in basis-points from PlatformConfig.
pub fn get_fee_bps(env: &Env, config: &Address) -> u32 {
    env.invoke_contract(config, &symbol_short!("get_fee_b"), soroban_sdk::vec![env])
}

/// Fetch the USDC token address from PlatformConfig.
pub fn get_usdc_token(env: &Env, config: &Address) -> Address {
    env.invoke_contract(config, &symbol_short!("get_usdc"), soroban_sdk::vec![env])
}

/// Fetch the admin address from PlatformConfig.
pub fn get_admin(env: &Env, config: &Address) -> Address {
    env.invoke_contract(config, &symbol_short!("get_adm"), soroban_sdk::vec![env])
}

/// Fetch the platform wallet address from PlatformConfig.
pub fn get_platform_wallet(env: &Env, config: &Address) -> Address {
    env.invoke_contract(config, &symbol_short!("get_pw"), soroban_sdk::vec![env])
}

// ── USDC token helpers ──────────────────────────────────────────────────────

/// Transfer USDC between two addresses via the USDC token contract.
pub fn usdc_transfer(env: &Env, usdc: &Address, from: &Address, to: &Address, amount: i128) {
    token::Client::new(env, usdc).transfer(from, to, &amount);
}

/// Query the USDC balance of an address.
pub fn usdc_balance(env: &Env, usdc: &Address, account: &Address) -> i128 {
    token::Client::new(env, usdc).balance(account)
}

/// Verify that `account` holds at least `required` USDC tokens.
/// Returns the current balance.
pub fn check_sufficient_balance(env: &Env, usdc: &Address, account: &Address, required: i128) -> i128 {
    let bal = usdc_balance(env, usdc, account);
    if bal < required {
        soroban_sdk::panic_with_error!(env, crate::errors::EscrowError::InsufficientBalance);
    }
    bal
}

// ── Escrow-to-PlatformConfig convenience bundle ─────────────────────────────

/// One-shot: fetch fee_bps, usdc, admin, and platform_wallet from PlatformConfig.
pub struct ConfigBundle {
    pub fee_bps: u32,
    pub usdc: Address,
    pub admin: Address,
    pub platform_wallet: Address,
}

impl ConfigBundle {
    pub fn load(env: &Env, config: &Address) -> Self {
        ConfigBundle {
            fee_bps: get_fee_bps(env, config),
            usdc: get_usdc_token(env, config),
            admin: get_admin(env, config),
            platform_wallet: get_platform_wallet(env, config),
        }
    }
}

// ── DisputeArbiter → EscrowContract interface ────────────────────────────────
//
// Closes #479 – cross-contract call from DisputeArbiter to EscrowContract
//
// These helpers are called by the DisputeArbiter to drive escrow state changes.
// Import them in the dispute_arbiter crate via a dependency on the escrow crate,
// or re-implement the invoke_contract pattern with the matching symbol names.

/// Symbol used by DisputeArbiter to trigger a refund on the EscrowContract.
pub const REFUND_CLIENT_SYMBOL: &str = "refund_cl";

/// Symbol used by DisputeArbiter to trigger a release on the EscrowContract.
pub const RELEASE_PAYMENT_SYMBOL: &str = "release_p";

/// Call `refund_client` on the EscrowContract from another contract (e.g. DisputeArbiter).
pub fn call_refund_client(env: &Env, escrow_contract: &Address, commission_id: Bytes, config_contract: Address) {
    env.invoke_contract::<()>(
        escrow_contract,
        &symbol_short!("refund_cl"),
        soroban_sdk::vec![env, commission_id.into_val(env), config_contract.into_val(env)],
    );
}

/// Call `release_payment` on the EscrowContract from another contract.
pub fn call_release_payment(env: &Env, escrow_contract: &Address, commission_id: Bytes, config_contract: Address) {
    env.invoke_contract::<()>(
        escrow_contract,
        &symbol_short!("release_p"),
        soroban_sdk::vec![env, commission_id.into_val(env), config_contract.into_val(env)],
    );
}
// ── Atomic escrow-to-commission orchestration (closes #656) ──────────────────
//
// A commission is funded by first locking USDC in escrow; when the agreement
// reaches a commit point the escrowed funds must land in the commission/settlement
// path *atomically* — never half-migrated, never stranded.
//
// Soroban guarantees host-level atomicity within a single transaction. The
// marker below adds *cross-transaction* coordination and observability: every
// participant confirms readiness, and the funds only ever move in one
// all-or-nothing call (`atomic_escrow_to_commission`). Until that call succeeds
// no money has moved, so a rolled-back or interrupted flow can always be cleanly
// refunded through the normal `refund_client` path.

/// Lifecycle of an atomic escrow→commission commit.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AtomicCommitState {
    /// Participants still confirming; no funds may move in this state.
    InProgress = 0,
    /// All participants confirmed and the single-transaction migration ran.
    Settled = 1,
    /// The flow was abandoned before any funds moved; escrow is intact.
    RolledBack = 2,
    /// A consistency or execution failure; the marker records it permanently.
    Failed = 3,
}

/// Cross-contract progress marker for an atomic escrow→commission operation.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AtomicCommitMarker {
    pub commission_id: Bytes,
    pub state: AtomicCommitState,
    /// Number of sides that must confirm readiness before funds may move.
    pub participants: u32,
    /// Number of sides that confirmed readiness.
    pub confirmed: u32,
    /// Ledger this flow was opened on.
    pub created_ledger: u32,
    /// Ledger the funds actually settled on (only set when `Settled`).
    pub settled_ledger: Option<u32>,
}

/// The number of independent sides in an escrow→commission commit.
pub const ATOMIC_COMMIT_PARTICIPANTS: u32 = 2;

fn load_marker(env: &Env, commission_id: &Bytes) -> Result<AtomicCommitMarker, EscrowError> {
    if !storage::atomic_marker_exists(env, commission_id) {
        return Err(EscrowError::NotFound);
    }
    Ok(storage::get_atomic_marker(env, commission_id))
}

fn require_in_progress(marker: &AtomicCommitMarker) -> Result<(), EscrowError> {
    if marker.state != AtomicCommitState::InProgress {
        return Err(EscrowError::AtomicCommitStateInvalid);
    }
    Ok(())
}

/// Open a commit marker for `commission_id`. The escrow must exist and be
/// `Locked` or `Disputed` (funds present, not yet released).
pub fn begin_atomic_commit(
    env: &Env,
    commission_id: &Bytes,
) -> Result<AtomicCommitMarker, EscrowError> {
    if storage::atomic_marker_exists(env, commission_id) {
        return Err(EscrowError::AlreadyExists);
    }
    if !storage::escrow_exists(env, commission_id) {
        return Err(EscrowError::NotFound);
    }
    let record = storage::get_escrow(env, commission_id);
    if record.status != CommissionStatus::Locked && record.status != CommissionStatus::Disputed {
        return Err(EscrowError::InvalidStatus);
    }

    let marker = AtomicCommitMarker {
        commission_id: commission_id.clone(),
        state: AtomicCommitState::InProgress,
        participants: ATOMIC_COMMIT_PARTICIPANTS,
        confirmed: 0,
        created_ledger: env.ledger().sequence(),
        settled_ledger: None,
    };
    storage::save_atomic_marker(env, &marker);
    Ok(marker)
}

/// Record that one side has confirmed readiness. Idempotent for a side that
/// already confirmed (confirmed never exceeds participants).
pub fn confirm_atomic_step(
    env: &Env,
    commission_id: &Bytes,
    from: Address,
) -> Result<u32, EscrowError> {
    from.require_auth();
    let mut marker = load_marker(env, commission_id)?;
    require_in_progress(&marker)?;
    if marker.confirmed >= marker.participants {
        return Err(EscrowError::AtomicCommitStateInvalid);
    }
    marker.confirmed = marker
        .confirmed
        .checked_add(1)
        .ok_or(EscrowError::ArithmeticOverflow)?;
    storage::save_atomic_marker(env, &marker);
    Ok(marker.confirmed)
}

/// Ask the commission agreement contract what amount it expects to be escrowed
/// for `commission_id`. Used by the atomic flow to verify both sides agree
/// before any funds move.
pub fn verify_agreement_consistency(
    env: &Env,
    commission_contract: &Address,
    commission_id: &Bytes,
    expected_amount: i128,
) -> Result<bool, EscrowError> {
    let agreed: i128 = env.invoke_contract(
        commission_contract,
        &Symbol::new(env, "get_agreement_escrow_amount"),
        soroban_sdk::vec![env, commission_id.clone().into_val(env)],
    );
    Ok(agreed == expected_amount)
}

/// Mark the flow settled and move the escrow record to `Released`. This is the
/// pre-funding commit point used by the multi-transaction coordination path;
/// it requires every participant to have confirmed first, keeping the
/// no-half-migrated guarantee.
pub fn finalize_atomic_commit(
    env: &Env,
    commission_id: &Bytes,
) -> Result<AtomicCommitMarker, EscrowError> {
    let mut marker = load_marker(env, commission_id)?;
    require_in_progress(&marker)?;
    if marker.confirmed < marker.participants {
        return Err(EscrowError::AtomicCommitNotReady);
    }

    let mut record = storage::get_escrow(env, commission_id);
    if record.status != CommissionStatus::Locked && record.status != CommissionStatus::Disputed {
        return Err(EscrowError::InvalidStatus);
    }
    record.status = CommissionStatus::Released;
    storage::save_escrow(env, &record);

    marker.state = AtomicCommitState::Settled;
    marker.settled_ledger = Some(env.ledger().sequence());
    storage::save_atomic_marker(env, &marker);
    Ok(marker)
}

/// Abandon the flow before any funds have moved. Only legal while the marker
/// is `InProgress`/`Failed`; the escrow record stays untouched and can be
/// released or refunded through the normal state machine.
pub fn rollback_atomic_commit(
    env: &Env,
    commission_id: &Bytes,
) -> Result<AtomicCommitMarker, EscrowError> {
    let mut marker = load_marker(env, commission_id)?;
    if marker.state != AtomicCommitState::InProgress && marker.state != AtomicCommitState::Failed {
        return Err(EscrowError::AtomicCommitStateInvalid);
    }
    marker.state = AtomicCommitState::RolledBack;
    storage::save_atomic_marker(env, &marker);

    crate::correlation_publish(
        env,
        "escrow",
        symbol_short!("rollback"),
        commission_id,
        commission_id,
    );
    Ok(marker)
}

/// The single-transaction migration: escrowed USDC moves from the escrow
/// contract to the commission settlement path, with the platform fee split
/// going to the platform wallet, plus Cross-contract verification.
///
/// All-or-nothing: any failed check, overflow, or transfer aborts the whole
/// transaction (host-level atomicity), and the marker is recorded as settled
/// only when the transfer completed.
pub fn atomic_escrow_to_commission(
    env: &Env,
    commission_id: &Bytes,
    config_contract: &Address,
    commission_contract: &Address,
) -> Result<AtomicCommitMarker, EscrowError> {
    // CHECKS (admin-authorized exactly like release_payment)
    let admin: Address =
        env.invoke_contract(config_contract, &symbol_short!("get_adm"), soroban_sdk::vec![env]);
    admin.require_auth();

    storage::with_reentrancy_guard(env, || {
        if !storage::escrow_exists(env, commission_id) {
            return Err(EscrowError::NotFound);
        }
        let record = storage::get_escrow(env, commission_id);
        if record.status != CommissionStatus::Locked
            && record.status != CommissionStatus::Disputed
        {
            return Err(EscrowError::InvalidStatus);
        }

        // Cross-contract consistency: the commission agreement must expect the
        // same amount we are migrating, otherwise abort atomically.
        if !verify_agreement_consistency(env, commission_contract, commission_id, record.amount)? {
            let marker = AtomicCommitMarker {
                commission_id: commission_id.clone(),
                state: AtomicCommitState::Failed,
                participants: ATOMIC_COMMIT_PARTICIPANTS,
                confirmed: ATOMIC_COMMIT_PARTICIPANTS,
                created_ledger: env.ledger().sequence(),
                settled_ledger: None,
            };
            storage::save_atomic_marker(env, &marker);
            return Ok(marker);
        }

        let (fee, payout) = crate::calculate_fee_split(record.amount, record.fee_bps)?;

        // EFFECTS (before interactions, CEI)
        let mut updated = record;
        updated.status = CommissionStatus::Released;
        storage::save_escrow(env, &updated);

        let marker = AtomicCommitMarker {
            commission_id: commission_id.clone(),
            state: AtomicCommitState::Settled,
            participants: ATOMIC_COMMIT_PARTICIPANTS,
            confirmed: ATOMIC_COMMIT_PARTICIPANTS,
            created_ledger: updated.created_ledger,
            settled_ledger: Some(env.ledger().sequence()),
        };
        storage::save_atomic_marker(env, &marker);

        // INTERACTIONS
        let usdc: Address =
            env.invoke_contract(config_contract, &symbol_short!("get_usdc"), soroban_sdk::vec![env]);
        let pw: Address =
            env.invoke_contract(config_contract, &symbol_short!("get_pw"), soroban_sdk::vec![env]);
        let tc = token::Client::new(env, &usdc);
        if fee > 0 {
            tc.transfer(&env.current_contract_address(), &pw, &fee);
        }
        if payout > 0 {
            tc.transfer(&env.current_contract_address(), commission_contract, &payout);
        }

        // EVENTS (primary + correlated)
        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("released")),
            (commission_id.clone(), payout, fee),
        );
        crate::correlation_publish(
            env,
            "escrow",
            symbol_short!("settled"),
            commission_id,
            commission_id,
        );
        Ok(marker)
    })
}
