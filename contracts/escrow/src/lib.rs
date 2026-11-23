//! Escrow Smart Contract
//!
//! Handles locking, releasing, refunding, and dispute escrow workflows for StellarAid.
//! Architecture Decision: [ADR-0002](../../docs/ADRs/0002-escrow-architecture-and-state-machine.md)
//! See also: [ADR-0007](../../docs/ADRs/0007-storage-data-model-and-ttl-management.md)

#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, token, Address, Bytes, Env};

pub mod cross_contract;
pub mod errors;
pub mod invariants;
pub mod storage;

pub use cross_contract::{AtomicCommitMarker, AtomicCommitState};
pub use invariants::InvariantError;

use errors::EscrowError;
use storage::{escrow_exists, get_escrow, save_escrow, CommissionStatus, EscrowRecord};



/// Emit a correlated event `(domain, action, "corr")` carrying a deterministic
/// correlation id derived from the shared operation key (#661).
///
/// Off-chain indexers join these events on the id + key to reconstruct the
/// cross-contract trace of a logical operation.
fn correlation_publish(
    env: &Env,
    domain: &str,
    action: soroban_sdk::Symbol,
    id: &Bytes,
    key: &Bytes,
) {
    let scope = shared::correlation::scope(env, domain);
    let cid = shared::correlation::CorrelationId::derive(env, &scope, &[id]);
    shared::correlation::publish(
        env,
        soroban_sdk::Symbol::new(env, domain),
        action,
        &cid,
        key,
    );
}

// ── Pause helpers (closes #594) ─────────────────────────────────────────────

/// Storage key for the escrow contract pause flag.
#[soroban_sdk::contracttype]
enum PauseKey {
    Paused,
    Admin,
    /// Admin address proposed via `transfer_admin`, awaiting `accept_admin`
    /// (closes #635 — two-step admin key rotation).
    PendingAdmin,
}

/// Require the escrow contract is not paused.
fn require_not_paused(env: &Env) -> Result<(), EscrowError> {
    let paused: bool = env
        .storage()
        .instance()
        .get(&PauseKey::Paused)
        .unwrap_or(false);
    if paused {
        Err(EscrowError::ContractPaused)
    } else {
        Ok(())
    }
}

/// Ledgers until an escrow record expires from persistent storage (~30 days at 6s/ledger).
/// Closes #487 – ledger-based TTL for escrow records.
/// Disputed escrows are extended with the configurable dispute-period TTL
/// instead (see [`EscrowContract::set_dispute_ttl_ledgers`], #586).
const ESCROW_TTL_LEDGERS: u32 = 432_000;

fn extend_escrow_ttl(env: &Env, record: &EscrowRecord, ledgers: u32) {
    use storage::DataKey;
    env.storage().persistent().extend_ttl(
        &DataKey::Escrow(record.commission_id.clone()),
        ledgers,
        ledgers,
    );
}

fn extend_escrow_ttl_default(env: &Env, record: &EscrowRecord) {
    extend_escrow_ttl(env, record, ESCROW_TTL_LEDGERS);
}

/// Overflow-safe fee split (#588).
///
/// Computes `(fee, payout)` from `amount` and `fee_bps` using checked
/// arithmetic. Any intermediate overflow returns [`EscrowError::ArithmeticOverflow`]
/// instead of silently zeroing the fee or aborting the transaction.
fn calculate_fee_split(amount: i128, fee_bps: u32) -> Result<(i128, i128), EscrowError> {
    let product = amount
        .checked_mul(fee_bps as i128)
        .ok_or(EscrowError::ArithmeticOverflow)?;
    let fee = product / 10_000;
    let payout = amount
        .checked_sub(fee)
        .ok_or(EscrowError::ArithmeticOverflow)?;
    Ok((fee, payout))
}

#[contract]
pub struct EscrowContract;

#[contractimpl]
impl EscrowContract {
    // ── Pause / Resume (closes #594) ────────────────────────────────────────

    /// Initialise the escrow admin. Must be called once after deployment.
    /// If `initialize` is never called, pause/unpause are unavailable.
    pub fn initialize(env: Env, admin: Address) -> Result<(), EscrowError> {
        admin.require_auth();
        if env.storage().instance().has(&PauseKey::Admin) {
            return Err(EscrowError::AlreadyExists);
        }
        env.storage().instance().set(&PauseKey::Admin, &admin);
        env.storage().instance().set(&PauseKey::Paused, &false);
        Ok(())
    }

    shared::impl_semver_queries!();

    /// Pause the escrow contract — blocks `create_escrow` and `refund_client`.
    /// Only callable by the escrow admin set during `initialize`.
    /// Closes #594.
    pub fn pause(env: Env, admin: Address) -> Result<(), EscrowError> {
        admin.require_auth();
        let stored: Address = env
            .storage()
            .instance()
            .get(&PauseKey::Admin)
            .ok_or(EscrowError::Unauthorized)?;
        if stored != admin {
            return Err(EscrowError::Unauthorized);
        }
        env.storage().instance().set(&PauseKey::Paused, &true);
        env.events()
            .publish((symbol_short!("esc"), symbol_short!("paused")), admin);
        Ok(())
    }

    /// Resume normal operations after a pause. Only callable by the escrow admin.
    /// Closes #594.
    pub fn unpause(env: Env, admin: Address) -> Result<(), EscrowError> {
        admin.require_auth();
        let stored: Address = env
            .storage()
            .instance()
            .get(&PauseKey::Admin)
            .ok_or(EscrowError::Unauthorized)?;
        if stored != admin {
            return Err(EscrowError::Unauthorized);
        }
        env.storage().instance().set(&PauseKey::Paused, &false);
        env.events()
            .publish((symbol_short!("esc"), symbol_short!("unpaused")), admin);
        Ok(())
    }

    /// Returns `true` when the contract is currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&PauseKey::Paused)
            .unwrap_or(false)
    }

    // ── Admin key rotation (closes #635) ─────────────────────────────────────
    //
    // Two-step transfer so rotating the admin key can never brick the
    // contract by pointing it at an unreachable/mistyped address: the
    // current admin proposes a successor, and only that successor's own
    // signature can complete the handover by calling `accept_admin`.

    /// Step 1 — current admin proposes `new_admin` as the successor.
    /// Does not change the active admin; `new_admin` must call
    /// `accept_admin` to complete the rotation.
    pub fn transfer_admin(env: Env, admin: Address, new_admin: Address) -> Result<(), EscrowError> {
        admin.require_auth();
        let stored: Address = env
            .storage()
            .instance()
            .get(&PauseKey::Admin)
            .ok_or(EscrowError::Unauthorized)?;
        if stored != admin {
            return Err(EscrowError::Unauthorized);
        }
        env.storage()
            .instance()
            .set(&PauseKey::PendingAdmin, &new_admin);
        env.events().publish(
            (symbol_short!("esc"), symbol_short!("adm_prop")),
            (admin, new_admin),
        );
        Ok(())
    }

    /// Step 2 — the proposed successor accepts and becomes the active admin.
    /// Only `new_admin`'s own signature can complete this; the current
    /// admin cannot force it through on the successor's behalf.
    pub fn accept_admin(env: Env, new_admin: Address) -> Result<(), EscrowError> {
        new_admin.require_auth();
        let pending: Address = env
            .storage()
            .instance()
            .get(&PauseKey::PendingAdmin)
            .ok_or(EscrowError::Unauthorized)?;
        if pending != new_admin {
            return Err(EscrowError::Unauthorized);
        }
        let previous: Address = env
            .storage()
            .instance()
            .get(&PauseKey::Admin)
            .ok_or(EscrowError::Unauthorized)?;
        env.storage().instance().set(&PauseKey::Admin, &new_admin);
        env.storage().instance().remove(&PauseKey::PendingAdmin);
        env.events().publish(
            (symbol_short!("esc"), symbol_short!("adm_rot")),
            (previous, new_admin),
        );
        Ok(())
    }

    /// Current admin withdraws an in-flight proposal without completing it.
    pub fn cancel_admin_transfer(env: Env, admin: Address) -> Result<(), EscrowError> {
        admin.require_auth();
        let stored: Address = env
            .storage()
            .instance()
            .get(&PauseKey::Admin)
            .ok_or(EscrowError::Unauthorized)?;
        if stored != admin {
            return Err(EscrowError::Unauthorized);
        }
        env.storage().instance().remove(&PauseKey::PendingAdmin);
        Ok(())
    }

    /// The proposed successor admin awaiting acceptance, if any.
    pub fn get_pending_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&PauseKey::PendingAdmin)
    }

    /// Closes #482 (CEI), #486 (events), #487 (TTL), #587 (reentrancy guard).
    /// Closes #594 (pause guard on create_escrow).
    /// CEI: Checks → Effects (save record) → Interactions (token transfer).
    pub fn create_escrow(
        env: Env,
        commission_id: Bytes,
        client: Address,
        artist: Address,
        amount: i128,
        config_contract: Address,
    ) -> Result<(), EscrowError> {
        client.require_auth();

        // CHECKS
        require_not_paused(&env)?;
        shared::circuit_breaker::require_not_halted(&env)?;
        if amount <= 0 {
            return Err(EscrowError::InvalidAmount);
        }
        if escrow_exists(&env, &commission_id) {
            return Err(EscrowError::AlreadyExists);
        }
        // Self-dealing guard (#801): a client must not escrow funds to themselves.
        if client == artist {
            return Err(EscrowError::InvalidAddress);
        }

        let fee_bps: u32 = env.invoke_contract(
            &config_contract,
            &symbol_short!("get_fee_b"),
            soroban_sdk::vec![&env],
        );
        let usdc_token: Address = env.invoke_contract(
            &config_contract,
            &symbol_short!("get_usdc"),
            soroban_sdk::vec![&env],
        );

        // EFFECTS + INTERACTIONS under re-entrancy guard (#587);
        // CEI: persist before the external token transfer.
        storage::with_reentrancy_guard(&env, || {
            let record = EscrowRecord {
                commission_id: commission_id.clone(),
                client: client.clone(),
                artist: artist.clone(),
                amount,
                fee_bps,
                status: CommissionStatus::Locked,
                created_ledger: env.ledger().sequence(),
                released_amount: 0,
            };
            save_escrow(&env, &record);
            extend_escrow_ttl_default(&env, &record);

            // INVARIANT: balance consistency — released starts at 0, amount is positive (#770)
            invariants::verify_balance_consistency(&env, &record).map_err(EscrowError::from)?;

            // INTERACTIONS – external call after effects
            token::Client::new(&env, &usdc_token).transfer(
                &client,
                &env.current_contract_address(),
                &amount,
            );

            // EVENT — additive funding notification (#978). The existing
            // `created` topic and payload are unchanged; `funded` carries the
            // extra facts (funder, asset, ledger) a client needs to notify an
            // artist without polling `get_escrow`.
            env.events().publish(
                (symbol_short!("escrow"), symbol_short!("created")),
                (commission_id.clone(), amount),
            );
            env.events().publish(
                (symbol_short!("escrow"), symbol_short!("funded")),
                (
                    commission_id.clone(),
                    client.clone(),
                    amount,
                    usdc_token.clone(),
                    env.ledger().sequence(),
                ),
            );
            correlation_publish(
                &env,
                "escrow",
                symbol_short!("created"),
                &commission_id,
                &commission_id,
            );
            Ok(())
        })
    }

    /// Closes #482 (CEI), #486 (events), #588 (overflow-safe fees), #587 (reentrancy guard).
    /// CEI: Checks → Effects (status update) → Interactions (transfers).
    pub fn release_payment(
        env: Env,
        commission_id: Bytes,
        config_contract: Address,
    ) -> Result<(), EscrowError> {
        // CHECKS
        shared::circuit_breaker::require_not_halted(&env)?;
        let mut r = get_escrow(&env, &commission_id);
        if r.status != CommissionStatus::Locked {
            return Err(EscrowError::InvalidStatus);
        }

        let admin: Address = env.invoke_contract(
            &config_contract,
            &symbol_short!("get_adm"),
            soroban_sdk::vec![&env],
        );
        admin.require_auth();
        let usdc: Address = env.invoke_contract(
            &config_contract,
            &symbol_short!("get_usdc"),
            soroban_sdk::vec![&env],
        );
        let pw: Address = env.invoke_contract(
            &config_contract,
            &symbol_short!("get_pw"),
            soroban_sdk::vec![&env],
        );

        let artist = r.artist.clone();

        storage::with_reentrancy_guard(&env, || {
            let (fee, payout) = calculate_fee_split(r.amount, r.fee_bps)?;

            // EFFECTS
            let prev_status = r.status;
            r.status = CommissionStatus::Released;

            // Invariant: validate state transition and balance consistency (#770)
            if !invariants::is_valid_transition(prev_status, r.status) {
                return Err(EscrowError::InvalidStateTransition);
            }
            invariants::verify_balance_consistency(&env, &r).map_err(EscrowError::from)?;
            save_escrow(&env, &r);

            // INTERACTIONS
            let tc = token::Client::new(&env, &usdc);
            tc.transfer(&env.current_contract_address(), &artist, &payout);
            tc.transfer(&env.current_contract_address(), &pw, &fee);

            // EVENT
            env.events().publish(
                (symbol_short!("escrow"), symbol_short!("released")),
                (commission_id.clone(), payout, fee),
            );
            Ok(())
        })
    }

    /// Closes #482 (CEI), #486 (events), #587 (reentrancy guard).
    /// Closes #594 (pause guard on refund_client).
    /// CEI: Checks → Effects (status update) → Interactions (transfer).
    pub fn refund_client(
        env: Env,
        commission_id: Bytes,
        config_contract: Address,
    ) -> Result<(), EscrowError> {
        // CHECKS
        require_not_paused(&env)?;
        shared::circuit_breaker::require_not_halted(&env)?;
        let mut r = get_escrow(&env, &commission_id);
        if r.status != CommissionStatus::Locked && r.status != CommissionStatus::Disputed {
            return Err(EscrowError::InvalidStatus);
        }
        let admin: Address = env.invoke_contract(
            &config_contract,
            &symbol_short!("get_adm"),
            soroban_sdk::vec![&env],
        );
        admin.require_auth();
        let usdc: Address = env.invoke_contract(
            &config_contract,
            &symbol_short!("get_usdc"),
            soroban_sdk::vec![&env],
        );

        let client = r.client.clone();
        let amount = r.amount;

        storage::with_reentrancy_guard(&env, || {
            // EFFECTS
            let prev_status = r.status;
            r.status = CommissionStatus::Refunded;

            // Invariant: validate state transition (#770)
            if !invariants::is_valid_transition(prev_status, r.status) {
                return Err(EscrowError::InvalidStateTransition);
            }
            save_escrow(&env, &r);

            // INTERACTIONS
            token::Client::new(&env, &usdc).transfer(
                &env.current_contract_address(),
                &client,
                &amount,
            );

            // EVENT
            env.events().publish(
                (symbol_short!("escrow"), symbol_short!("refunded")),
                (commission_id.clone(), client.clone(), amount),
            );
            Ok(())
        })
    }

    /// Closes #482 (CEI), #486 (events).
    pub fn expire_escrow(
        env: Env,
        commission_id: Bytes,
        expiry_ledger: u32,
    ) -> Result<(), EscrowError> {
        // CHECKS
        shared::circuit_breaker::require_not_halted(&env)?;
        let mut r = get_escrow(&env, &commission_id);
        if r.status != CommissionStatus::Locked {
            return Err(EscrowError::InvalidStatus);
        }
        if env.ledger().sequence() < expiry_ledger {
            return Err(EscrowError::NotExpired);
        }

        // EFFECTS
        let prev_status = r.status;
        r.status = CommissionStatus::Expired;

        // Invariant: validate state transition (#770)
        if !invariants::is_valid_transition(prev_status, r.status) {
            return Err(EscrowError::InvalidStateTransition);
        }
        save_escrow(&env, &r);

        // EVENT
        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("expired")),
            (commission_id, expiry_ledger),
        );
        Ok(())
    }

    /// Closes #482 (CEI), #486 (events), #586 (dispute-period TTL extension).
    pub fn open_dispute(
        env: Env,
        commission_id: Bytes,
        initiator: Address,
    ) -> Result<(), EscrowError> {
        initiator.require_auth();

        // CHECKS
        shared::circuit_breaker::require_not_halted(&env)?;
        let mut r = get_escrow(&env, &commission_id);
        if r.status == CommissionStatus::Disputed {
            return Err(EscrowError::DisputeAlreadyOpen);
        }
        if r.status != CommissionStatus::Locked {
            return Err(EscrowError::InvalidStatus);
        }
        if initiator != r.client && initiator != r.artist {
            return Err(EscrowError::Unauthorized);
        }

        // EFFECTS – extend the record TTL with the dispute-period length so the
        // escrow cannot expire mid-arbitration (#586).
        let prev_status = r.status;
        r.status = CommissionStatus::Disputed;

        // Invariant: validate state transition (#770)
        if !invariants::is_valid_transition(prev_status, r.status) {
            return Err(EscrowError::InvalidStateTransition);
        }
        save_escrow(&env, &r);
        extend_escrow_ttl(&env, &r, storage::get_dispute_ttl_ledgers(&env));

        // EVENT
        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("disputed")),
            (commission_id, initiator),
        );
        Ok(())
    }

    /// Configure the dispute-period TTL extension in ledgers (#586).
    ///
    /// Only the platform admin (as resolved from `config_contract`) may call
    /// this. The value is used by `open_dispute` to extend a disputed escrow's
    /// persistent-storage TTL so it survives arbitration.
    ///
    /// Minimum: `ESCROW_TTL_LEDGERS` (432,000 ledgers). Values below the minimum
    /// are rejected with `EscrowError::InvalidTtl`.
    pub fn set_dispute_ttl_ledgers(
        env: Env,
        config_contract: Address,
        ledgers: u32,
    ) -> Result<(), EscrowError> {
        let admin: Address = env.invoke_contract(
            &config_contract,
            &symbol_short!("get_adm"),
            soroban_sdk::vec![&env],
        );
        admin.require_auth();

        // CHECKS
        if ledgers == 0 || ledgers < ESCROW_TTL_LEDGERS {
            return Err(EscrowError::InvalidTtl);
        }

        // EFFECTS
        storage::set_dispute_ttl_ledgers(&env, ledgers);

        // EVENT
        env.events()
            .publish((symbol_short!("ttl"), symbol_short!("updated")), ledgers);
        Ok(())
    }

    /// Returns the currently configured dispute-period TTL in ledgers (#586).
    pub fn get_dispute_ttl_ledgers(env: Env) -> u32 {
        storage::get_dispute_ttl_ledgers(&env)
    }

    /// Partially release funds for a completed milestone.
    ///
    /// Releases `release_amount` USDC from the escrow to the artist (minus
    /// platform fee). The escrow remains in `Locked` (or `PartiallyReleased`)
    /// state until the full amount is paid out, at which point it transitions
    /// to `Released`.
    ///
    /// Closes #601 – escrow partial release for milestone-based work.
    ///
    /// # Errors
    /// - [`EscrowError::InvalidStatus`] if escrow is not Locked or PartiallyReleased
    /// - [`EscrowError::InvalidAmount`] if `release_amount <= 0` or exceeds remaining
    /// - [`EscrowError::Unauthorized`] if caller is not the admin
    pub fn partial_release(
        env: Env,
        commission_id: Bytes,
        release_amount: i128,
        config_contract: Address,
    ) -> Result<(), EscrowError> {
        // CHECKS
        shared::circuit_breaker::require_not_halted(&env)?;
        let mut r = get_escrow(&env, &commission_id);
        if r.status != CommissionStatus::Locked && r.status != CommissionStatus::PartiallyReleased {
            return Err(EscrowError::InvalidStatus);
        }
        if release_amount <= 0 {
            return Err(EscrowError::InvalidAmount);
        }

        let remaining = r
            .amount
            .checked_sub(r.released_amount)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        if release_amount > remaining {
            return Err(EscrowError::InvalidAmount);
        }

        let admin: Address = env.invoke_contract(
            &config_contract,
            &symbol_short!("get_adm"),
            soroban_sdk::vec![&env],
        );
        admin.require_auth();
        let usdc: Address = env.invoke_contract(
            &config_contract,
            &symbol_short!("get_usdc"),
            soroban_sdk::vec![&env],
        );
        let pw: Address = env.invoke_contract(
            &config_contract,
            &symbol_short!("get_pw"),
            soroban_sdk::vec![&env],
        );

        let artist = r.artist.clone();

        storage::with_reentrancy_guard(&env, || {
            let (fee, payout) = calculate_fee_split(release_amount, r.fee_bps)?;

            // EFFECTS – update released_amount and status before external calls
            let prev_status = r.status;
            r.released_amount = r
                .released_amount
                .checked_add(release_amount)
                .ok_or(EscrowError::ArithmeticOverflow)?;

            let new_remaining = r
                .amount
                .checked_sub(r.released_amount)
                .ok_or(EscrowError::ArithmeticOverflow)?;

            r.status = if new_remaining == 0 {
                CommissionStatus::Released
            } else {
                CommissionStatus::PartiallyReleased
            };

            // Invariant: validate state transition and balance consistency (#770)
            if !invariants::is_valid_transition(prev_status, r.status) {
                return Err(EscrowError::InvalidStateTransition);
            }
            invariants::verify_balance_consistency(&env, &r).map_err(EscrowError::from)?;
            save_escrow(&env, &r);
            extend_escrow_ttl_default(&env, &r);

            // INTERACTIONS
            let tc = token::Client::new(&env, &usdc);
            tc.transfer(&env.current_contract_address(), &artist, &payout);
            tc.transfer(&env.current_contract_address(), &pw, &fee);

            // EVENT
            env.events().publish(
                (symbol_short!("escrow"), symbol_short!("partial")),
                (
                    commission_id.clone(),
                    release_amount,
                    payout,
                    fee,
                    r.released_amount,
                ),
            );
            Ok(())
        })
    }

    /// Auto-release remaining escrow funds when a deadline has passed.
    ///
    /// If `auto_release_ledger` has been reached and the escrow is still in
    /// `Locked` or `PartiallyReleased` state, the remaining balance is released
    /// to the artist. This implements the auto-release on deadline requirement
    /// from issue #601.
    ///
    /// Closes #601 – auto-release on deadline.
    pub fn auto_release_on_deadline(
        env: Env,
        commission_id: Bytes,
        auto_release_ledger: u32,
        config_contract: Address,
    ) -> Result<(), EscrowError> {
        // CHECKS
        shared::circuit_breaker::require_not_halted(&env)?;
        let mut r = get_escrow(&env, &commission_id);
        if r.status != CommissionStatus::Locked && r.status != CommissionStatus::PartiallyReleased {
            return Err(EscrowError::InvalidStatus);
        }
        if env.ledger().sequence() < auto_release_ledger {
            return Err(EscrowError::NotExpired);
        }

        let remaining = r
            .amount
            .checked_sub(r.released_amount)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        if remaining <= 0 {
            return Err(EscrowError::InvalidAmount);
        }

        let usdc: Address = env.invoke_contract(
            &config_contract,
            &symbol_short!("get_usdc"),
            soroban_sdk::vec![&env],
        );
        let pw: Address = env.invoke_contract(
            &config_contract,
            &symbol_short!("get_pw"),
            soroban_sdk::vec![&env],
        );

        let artist = r.artist.clone();

        storage::with_reentrancy_guard(&env, || {
            let (fee, payout) = calculate_fee_split(remaining, r.fee_bps)?;

            // EFFECTS
            let prev_status = r.status;
            r.released_amount = r.amount;
            r.status = CommissionStatus::Released;

            // INVARIANT: validate the state transition is legal (#770)
            if !invariants::is_valid_transition(prev_status, r.status) {
                return Err(invariants::InvariantError::InvalidStateTransition.into());
            }
            // INVARIANT: released must equal amount when fully released
            if r.released_amount != r.amount {
                return Err(invariants::InvariantError::BalanceMismatch.into());
            }
            save_escrow(&env, &r);
            extend_escrow_ttl_default(&env, &r);

            // INTERACTIONS
            let tc = token::Client::new(&env, &usdc);
            if payout > 0 {
                tc.transfer(&env.current_contract_address(), &artist, &payout);
            }
            if fee > 0 {
                tc.transfer(&env.current_contract_address(), &pw, &fee);
            }

            // EVENT
            env.events().publish(
                (symbol_short!("escrow"), symbol_short!("autorls")),
                (
                    commission_id.clone(),
                    auto_release_ledger,
                    remaining,
                    payout,
                    fee,
                ),
            );
            Ok(())
        })
    }

    /// Settle a cancelled commission (#605).
    ///
    /// The split is computed off-contract by the commission agreement's
    /// pro-rata settlement and passed in; the two amounts must account for the
    /// escrowed total exactly, so no dust can be stranded. The platform fee is
    /// charged only on the artist's share — the client's refund is not taxed.
    ///
    /// CEI: Checks → Effects (status update) → Interactions (transfers).
    /// Closes #770 — balance and state consistency invariants are verified.
    pub fn cancel_escrow(
        env: Env,
        commission_id: Bytes,
        config_contract: Address,
        artist_amount: i128,
        client_refund: i128,
    ) -> Result<(), EscrowError> {
        // CHECKS
        shared::circuit_breaker::require_not_halted(&env)?;
        let mut r = get_escrow(&env, &commission_id);
        if r.status != CommissionStatus::Locked
            && r.status != CommissionStatus::Disputed
            && r.status != CommissionStatus::PartiallyReleased
        {
            return Err(EscrowError::InvalidStatus);
        }
        if artist_amount < 0 || client_refund < 0 {
            return Err(EscrowError::InvalidAmount);
        }
        let total = artist_amount
            .checked_add(client_refund)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        if total != r.amount {
            return Err(EscrowError::InvalidSplit);
        }

        let admin: Address = env.invoke_contract(
            &config_contract,
            &symbol_short!("get_adm"),
            soroban_sdk::vec![&env],
        );
        admin.require_auth();
        let usdc: Address = env.invoke_contract(
            &config_contract,
            &symbol_short!("get_usdc"),
            soroban_sdk::vec![&env],
        );
        let pw: Address = env.invoke_contract(
            &config_contract,
            &symbol_short!("get_pw"),
            soroban_sdk::vec![&env],
        );

        let artist = r.artist.clone();
        let client = r.client.clone();

        storage::with_reentrancy_guard(&env, || {
            let (fee, payout) = calculate_fee_split(artist_amount, r.fee_bps)?;

            // EFFECTS
            let prev_status = r.status;
            r.status = CommissionStatus::Cancelled;

            // Invariant: validate the state transition is legal (#770)
            if !invariants::is_valid_transition(prev_status, r.status) {
                return Err(invariants::InvariantError::InvalidStateTransition.into());
            }
            save_escrow(&env, &r);

            // INTERACTIONS
            let tc = token::Client::new(&env, &usdc);
            if payout > 0 {
                tc.transfer(&env.current_contract_address(), &artist, &payout);
            }
            if fee > 0 {
                tc.transfer(&env.current_contract_address(), &pw, &fee);
            }
            if client_refund > 0 {
                tc.transfer(&env.current_contract_address(), &client, &client_refund);
            }

            // EVENT
            env.events().publish(
                (symbol_short!("escrow"), symbol_short!("cancelled")),
                (commission_id.clone(), payout, fee, client_refund),
            );
            Ok(())
        })
    }

    pub fn get_escrow(env: Env, commission_id: Bytes) -> Result<EscrowRecord, EscrowError> {
        if !escrow_exists(&env, &commission_id) {
            return Err(EscrowError::NotFound);
        }
        Ok(storage::get_escrow(&env, &commission_id))
    }

    /// Read-only amount escrowed for **one** commission (closes #1093).
    ///
    /// Cross-contract callers such as DisputeArbiter must use this instead of
    /// treating the escrow contract's whole token balance as this commission's
    /// funds. No auth required. Unknown ids return `EscrowError::NotFound`.
    /// The value is the originally escrowed amount, independent of status
    /// (Locked, PartiallyReleased, Expired, Refunded, ...).
    pub fn get_escrow_amount(env: Env, commission_id: Bytes) -> Result<i128, EscrowError> {
        Ok(get_escrow(&env, &commission_id)?.amount)
    }

    // ── Atomic escrow-to-commission flow (closes #656) ────────────────────
    //
    // Public entry points over the orchestration in `cross_contract`. The
    // multi-step begin/confirm/finalize/rollback markers coordinate the two
    // sides across transactions; `atomic_escrow_to_commission` performs the
    // single all-or-nothing fund migration guard with the host's atomicity.

    /// Open a commit marker for an escrow that will migrate into a commission.
    pub fn begin_atomic_commit(
        env: Env,
        commission_id: Bytes,
    ) -> Result<AtomicCommitMarker, EscrowError> {
        cross_contract::begin_atomic_commit(&env, &commission_id)
    }

    /// Record one participant’s readiness confirmation.
    pub fn confirm_atomic_step(
        env: Env,
        commission_id: Bytes,
        from: Address,
    ) -> Result<u32, EscrowError> {
        cross_contract::confirm_atomic_step(&env, &commission_id, from)
    }

    /// Aware both sides confirmed: mark the flow settled and the escrow released.
    pub fn finalize_atomic_commit(
        env: Env,
        commission_id: Bytes,
    ) -> Result<AtomicCommitMarker, EscrowError> {
        cross_contract::finalize_atomic_commit(&env, &commission_id)
    }

    /// Abandon the flow before any funds moved; escrow stays intact.
    pub fn rollback_atomic_commit(
        env: Env,
        commission_id: Bytes,
    ) -> Result<AtomicCommitMarker, EscrowError> {
        cross_contract::rollback_atomic_commit(&env, &commission_id)
    }

    /// Read the current commit marker.
    ///
    /// Refreshes the marker's TTL and resolves an `InProgress` marker that has
    /// outlived `ATOMIC_COMMIT_MAX_LIFETIME_LEDGERS` to the terminal
    /// `RolledBack` state (#988).
    pub fn get_atomic_commit(
        env: Env,
        commission_id: Bytes,
    ) -> Result<AtomicCommitMarker, EscrowError> {
        cross_contract::get_atomic_commit(&env, &commission_id)
    }

    /// Cross-contract consistency probe: does the commission agreement expect
    /// exactly `expected_amount` escrowed for this id?
    pub fn verify_agreement_consistency(
        env: Env,
        commission_contract: Address,
        commission_id: Bytes,
        expected_amount: i128,
    ) -> Result<bool, EscrowError> {
        cross_contract::verify_agreement_consistency(
            &env,
            &commission_contract,
            &commission_id,
            expected_amount,
        )
    }

    /// Atomic, single-transaction escrow→commission migration.
    ///
    /// Verifies the commission side agrees with the escrowed amount, then moves
    /// the escrowed balance (minus the platform fee) to the commission contract
    /// and the fee to the platform wallet. Any failed check or transfer aborts
    /// the whole transaction — nothing is left half-migrated (#656, #770).
    pub fn atomic_escrow_to_commission(
        env: Env,
        commission_id: Bytes,
        config_contract: Address,
        commission_contract: Address,
    ) -> Result<AtomicCommitMarker, EscrowError> {
        cross_contract::atomic_escrow_to_commission(
            &env,
            &commission_id,
            &config_contract,
            &commission_contract,
        )
    }

    // ── Invariant checks (#770) ──────────────────────────────────────────────

    /// Verify balance and state invariants for a single escrow record.
    ///
    /// Checks that `released_amount + remaining == amount` and that the stored
    /// status is one of the valid terminal/intermediate states. Does not touch
    /// the token contract — for on-chain balance verification use
    /// [`Self::verify_contract_balance`].
    pub fn verify_escrow_invariants(env: Env, commission_id: Bytes) -> Result<bool, EscrowError> {
        let record = get_escrow(&env, &commission_id);
        invariants::verify_balance_consistency(&env, &record)?;
        Ok(true)
    }

    /// Cross-contract escrow amount verification (#770).
    ///
    /// Queries the commission agreement contract's `get_agreement_escrow_amount`
    /// and checks the result matches the escrowed record's `amount`.
    pub fn verify_escrow_amount(
        env: Env,
        commission_id: Bytes,
        commission_contract: Address,
    ) -> Result<bool, EscrowError> {
        let record = get_escrow(&env, &commission_id);
        let result =
            invariants::verify_escrow_amount(&env, &commission_contract, &commission_id, &record)?;
        Ok(result.is_some())
    }

    // ── Health monitoring (#678) and gradual rollout (#684) ──────────────
    //
    // The observability surface is four entry points:
    //   get_health_snapshot  - read-only; replaces detect_anomaly,
    //                          get_health_metrics, get_sla_targets and
    //                          get_alert_config (and embeds get_rollout_state)
    //   health_check         - classify + rate-limited `hlth_alrt` event only
    //   set_alert_config     - admin: alert thresholds
    //   report_outcome       - admin: record one ok/error sample
    // Health monitoring never changes deployment state. Rollback is the
    // separate, explicit `evaluate_auto_rollback` / `trigger_rollback` path.

    /// One-call operator view: anomaly flag, status, metrics, SLA targets,
    /// alert config and rollout state, all read from the same ledger.
    /// Read-only (no alert event, no rollback), so it is safe to simulate.
    pub fn get_health_snapshot(env: Env) -> shared::health::HealthSnapshot {
        shared::health::health_snapshot(&env)
    /// One-call operator view: anomaly flag, status, metrics, SLA targets,
    /// alert config and rollout state, all read from the same ledger.
    /// Read-only (no alert event, no auto-rollback), so it is safe to simulate.
    /// Prefer this over calling detect_anomaly, get_health_metrics,
    /// get_sla_targets, get_alert_config and get_rollout_state separately.
    pub fn get_health_snapshot(env: Env) -> shared::health::HealthSnapshot {
        shared::health::health_snapshot(&env)
    }
    pub fn health_check(env: Env) -> shared::health::HealthReport {
        let report = shared::health::health_check(&env);
        if report.anomaly {
            shared::rollout::maybe_auto_rollback(&env);
        }
        report
    }
    pub fn get_health_metrics(env: Env) -> shared::health::HealthMetrics {
        shared::health::get_metrics(&env)
    }
    /// Classify current health and emit a `hlth_alrt` event if one is due.
    ///
    /// Has no rollout side effect: it never triggers a rollback. Call
    /// `evaluate_auto_rollback` to act on an anomaly.
    pub fn health_check(env: Env) -> shared::health::HealthReport {
        shared::health::health_check(&env)
    }
    pub fn set_alert_config(env: Env, admin: Address, config: shared::health::AlertConfig) {
        admin.require_auth();
        shared::health::set_alert_config(&env, config);
    }
    /// Record one invocation outcome sample (`ok = false` counts as an error).
    /// Replaces the former `report_ok` / `report_error` pair.
    pub fn report_outcome(env: Env, admin: Address, ok: bool) {
        admin.require_auth();
        shared::health::record_outcome(&env, ok);
    }
    pub fn set_feature_flag(env: Env, admin: Address, flag: soroban_sdk::Symbol, enabled: bool) {
        admin.require_auth();
        shared::rollout::set_feature_flag(&env, &flag, enabled);
    }
    pub fn is_feature_enabled(env: Env, flag: soroban_sdk::Symbol) -> bool {
        shared::rollout::is_feature_enabled(&env, &flag)
    }
    pub fn set_canary_deployment(
        env: Env,
        admin: Address,
        canary: Address,
        stable: Address,
        canary_bps: u32,
    ) {
        admin.require_auth();
        shared::rollout::set_canary_deployment(&env, canary, stable, canary_bps);
    }
    pub fn route_to_canary(env: Env, caller: Address) -> bool {
        shared::rollout::route_to_canary(&env, &caller)
    }
    pub fn get_rollout_state(env: Env) -> shared::rollout::RolloutState {
        shared::rollout::get_state(&env)
    }
    pub fn set_rollback_trigger(env: Env, admin: Address, error_bps: u32) {
        admin.require_auth();
        shared::rollout::set_rollback_trigger(&env, error_bps);
    }
    pub fn should_rollback(env: Env) -> bool {
        shared::rollout::should_rollback(&env)
    }
    /// Explicitly apply the automatic rollback if its trigger has fired.
    ///
    /// This used to run as a hidden side effect of `health_check`. It is now
    /// the only automatic path that changes deployment state. Returns `true`
    /// when the contract is (now) rolled back. Permissionless, like
    /// `health_check` was: it acts only when the configured trigger is met.
    pub fn evaluate_auto_rollback(env: Env) -> bool {
        shared::rollout::maybe_auto_rollback(&env)
    }
    pub fn trigger_rollback(env: Env, admin: Address) {
        admin.require_auth();
        shared::rollout::trigger_rollback(&env, &admin);
    }

    // ── Circuit breaker (#771) ────────────────────────────────────────────

    /// Manually halt the escrow contract (admin only).
    ///
    /// Halting prevents `create_escrow`, `release_payment`, `refund_client`,
    /// `partial_release`, `cancel_escrow`, `auto_release_on_deadline`, and
    /// `open_dispute` from executing. The pause flag and time-locked recovery
    /// from [`shared::pause`] are reused.
    pub fn halt(env: Env, admin: Address) -> Result<(), EscrowError> {
        admin.require_auth();
        let stored: Address = env
            .storage()
            .instance()
            .get(&PauseKey::Admin)
            .ok_or(EscrowError::Unauthorized)?;
        if stored != admin {
            return Err(EscrowError::Unauthorized);
        }
        shared::circuit_breaker::halt(&env, &admin)
            .map_err(|_| EscrowError::CircuitBreakerTripped)?;
        Ok(())
    }

    /// Manually resume the escrow contract after a halt (admin only).
    ///
    /// If a time-locked recovery was scheduled via `schedule_recovery`, the
    /// lock must have matured first.
    pub fn resume(env: Env, admin: Address) -> Result<(), EscrowError> {
        admin.require_auth();
        let stored: Address = env
            .storage()
            .instance()
            .get(&PauseKey::Admin)
            .ok_or(EscrowError::Unauthorized)?;
        if stored != admin {
            return Err(EscrowError::Unauthorized);
        }
        shared::circuit_breaker::resume(&env, &admin)
            .map_err(|_| EscrowError::CircuitBreakerTripped)?;
        Ok(())
    }

    /// `true` when the circuit breaker is currently tripped.
    pub fn is_halted(env: Env) -> bool {
        shared::circuit_breaker::is_halted(&env)
    }

    /// Current snapshot of the circuit-breaker state: halt flag, error rate,
    /// threshold, and any pending recovery time-lock.
    pub fn get_circuit_breaker_state(env: Env) -> shared::circuit_breaker::CircuitBreakerState {
        shared::circuit_breaker::get_state(&env)
    }

    /// Configure the automatic-halt threshold (admin only).
    ///
    /// `halt_error_bps` is the error rate (in 1/100ths of a percent, 0–10 000)
    /// at or above which [`maybe_auto_halt`](shared::circuit_breaker::maybe_auto_halt)
    /// will pause the contract.
    pub fn set_circuit_breaker_config(
        env: Env,
        admin: Address,
        halt_error_bps: u32,
    ) -> Result<(), EscrowError> {
        admin.require_auth();
        let stored: Address = env
            .storage()
            .instance()
            .get(&PauseKey::Admin)
            .ok_or(EscrowError::Unauthorized)?;
        if stored != admin {
            return Err(EscrowError::Unauthorized);
        }
        shared::circuit_breaker::set_config(
            &env,
            &shared::circuit_breaker::CircuitBreakerConfig {
                admin: admin.clone(),
                halt_error_bps,
            },
        );
        Ok(())
    }
}

#[cfg(test)]
mod atomic_flow_tests;
#[cfg(test)]
mod cancellation_tests;
#[cfg(test)]
mod correlation_tests;
#[cfg(test)]
mod dispute_tests;
#[cfg(test)]
mod fee_math_tests;
#[cfg(test)]
mod funding_event_tests;
#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod invariant_tests;
#[cfg(test)]
mod refund_tests;
#[cfg(test)]
mod storage_edge_tests;
#[cfg(test)]
mod tests;
