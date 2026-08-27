//! Escrow Smart Contract
//!
//! Handles locking, releasing, refunding, and dispute escrow workflows for StellarAid.
//! Architecture Decision: [ADR-0002](../../docs/ADRs/0002-escrow-architecture-and-state-machine.md)
//! See also: [ADR-0007](../../docs/ADRs/0007-storage-data-model-and-ttl-management.md)

#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, token, Address, Bytes, Env};

pub mod errors;
pub mod storage;

use errors::EscrowError;
use storage::{CommissionStatus, EscrowRecord, escrow_exists, get_escrow, save_escrow};

// ── Pause helpers (closes #594) ─────────────────────────────────────────────

/// Storage key for the escrow contract pause flag.
#[soroban_sdk::contracttype]
enum PauseKey {
    Paused,
    Admin,
}

/// Require the escrow contract is not paused.
fn require_not_paused(env: &Env) -> Result<(), EscrowError> {
    let paused: bool = env.storage().instance().get(&PauseKey::Paused).unwrap_or(false);
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

    /// Pause the escrow contract — blocks `create_escrow` and `refund_client`.
    /// Only callable by the escrow admin set during `initialize`.
    /// Closes #594.
    pub fn pause(env: Env, admin: Address) -> Result<(), EscrowError> {
        admin.require_auth();
        let stored: Address = env.storage().instance()
            .get(&PauseKey::Admin)
            .ok_or(EscrowError::Unauthorized)?;
        if stored != admin {
            return Err(EscrowError::Unauthorized);
        }
        env.storage().instance().set(&PauseKey::Paused, &true);
        env.events().publish(
            (symbol_short!("esc"), symbol_short!("paused")),
            admin,
        );
        Ok(())
    }

    /// Resume normal operations after a pause. Only callable by the escrow admin.
    /// Closes #594.
    pub fn unpause(env: Env, admin: Address) -> Result<(), EscrowError> {
        admin.require_auth();
        let stored: Address = env.storage().instance()
            .get(&PauseKey::Admin)
            .ok_or(EscrowError::Unauthorized)?;
        if stored != admin {
            return Err(EscrowError::Unauthorized);
        }
        env.storage().instance().set(&PauseKey::Paused, &false);
        env.events().publish(
            (symbol_short!("esc"), symbol_short!("unpaused")),
            admin,
        );
        Ok(())
    }

    /// Returns `true` when the contract is currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage().instance().get(&PauseKey::Paused).unwrap_or(false)
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
        if amount <= 0 { return Err(EscrowError::InvalidAmount); }
        if escrow_exists(&env, &commission_id) { return Err(EscrowError::AlreadyExists); }

        let fee_bps: u32 = env.invoke_contract(
            &config_contract, &symbol_short!("get_fee_b"), soroban_sdk::vec![&env],
        );
        let usdc_token: Address = env.invoke_contract(
            &config_contract, &symbol_short!("get_usdc"), soroban_sdk::vec![&env],
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
            };
            save_escrow(&env, &record);
            extend_escrow_ttl_default(&env, &record);

            // INTERACTIONS – external call after effects
            token::Client::new(&env, &usdc_token).transfer(
                &client, &env.current_contract_address(), &amount,
            );

            // EVENT
            env.events().publish(
                (symbol_short!("escrow"), symbol_short!("created")),
                (commission_id.clone(), amount),
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
        let mut r = get_escrow(&env, &commission_id);
        if r.status != CommissionStatus::Locked { return Err(EscrowError::InvalidStatus); }

        let admin: Address = env.invoke_contract(&config_contract, &symbol_short!("get_adm"), soroban_sdk::vec![&env]);
        admin.require_auth();
        let usdc: Address = env.invoke_contract(&config_contract, &symbol_short!("get_usdc"), soroban_sdk::vec![&env]);
        let pw: Address = env.invoke_contract(&config_contract, &symbol_short!("get_pw"), soroban_sdk::vec![&env]);

        let artist = r.artist.clone();

        storage::with_reentrancy_guard(&env, || {
            let (fee, payout) = calculate_fee_split(r.amount, r.fee_bps)?;

            // EFFECTS
            r.status = CommissionStatus::Released;
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
        let mut r = get_escrow(&env, &commission_id);
        if r.status != CommissionStatus::Locked && r.status != CommissionStatus::Disputed {
            return Err(EscrowError::InvalidStatus);
        }
        let admin: Address = env.invoke_contract(&config_contract, &symbol_short!("get_adm"), soroban_sdk::vec![&env]);
        admin.require_auth();
        let usdc: Address = env.invoke_contract(&config_contract, &symbol_short!("get_usdc"), soroban_sdk::vec![&env]);

        let client = r.client.clone();
        let amount = r.amount;

        storage::with_reentrancy_guard(&env, || {
            // EFFECTS
            r.status = CommissionStatus::Refunded;
            save_escrow(&env, &r);

            // INTERACTIONS
            token::Client::new(&env, &usdc).transfer(&env.current_contract_address(), &client, &amount);

            // EVENT
            env.events().publish(
                (symbol_short!("escrow"), symbol_short!("refunded")),
                (commission_id.clone(), client.clone(), amount),
            );
            Ok(())
        })
    }

    /// Closes #482 (CEI), #486 (events).
    pub fn expire_escrow(env: Env, commission_id: Bytes, expiry_ledger: u32) -> Result<(), EscrowError> {
        // CHECKS
        let mut r = get_escrow(&env, &commission_id);
        if r.status != CommissionStatus::Locked { return Err(EscrowError::InvalidStatus); }
        if env.ledger().sequence() < expiry_ledger { return Err(EscrowError::NotExpired); }

        // EFFECTS
        r.status = CommissionStatus::Expired;
        save_escrow(&env, &r);

        // EVENT
        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("expired")),
            (commission_id, expiry_ledger),
        );
        Ok(())
    }

    /// Closes #482 (CEI), #486 (events), #586 (dispute-period TTL extension).
    pub fn open_dispute(env: Env, commission_id: Bytes, initiator: Address) -> Result<(), EscrowError> {
        initiator.require_auth();

        // CHECKS
        let mut r = get_escrow(&env, &commission_id);
        if r.status == CommissionStatus::Disputed { return Err(EscrowError::DisputeAlreadyOpen); }
        if r.status != CommissionStatus::Locked { return Err(EscrowError::InvalidStatus); }
        if initiator != r.client && initiator != r.artist { return Err(EscrowError::Unauthorized); }

        // EFFECTS – extend the record TTL with the dispute-period length so the
        // escrow cannot expire mid-arbitration (#586).
        r.status = CommissionStatus::Disputed;
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
    pub fn set_dispute_ttl_ledgers(
        env: Env,
        config_contract: Address,
        ledgers: u32,
    ) -> Result<(), EscrowError> {
        let admin: Address = env.invoke_contract(
            &config_contract, &symbol_short!("get_adm"), soroban_sdk::vec![&env],
        );
        admin.require_auth();

        // CHECKS
        if ledgers == 0 || ledgers < ESCROW_TTL_LEDGERS {
            return Err(EscrowError::InvalidAmount);
        }

        // EFFECTS
        storage::set_dispute_ttl_ledgers(&env, ledgers);

        // EVENT
        env.events().publish(
            (symbol_short!("ttl"), symbol_short!("updated")),
            ledgers,
        );
        Ok(())
    }

    /// Returns the currently configured dispute-period TTL in ledgers (#586).
    pub fn get_dispute_ttl_ledgers(env: Env) -> u32 {
        storage::get_dispute_ttl_ledgers(&env)
    }

    /// Settle a cancelled commission (#605).
    ///
    /// The split is computed off-contract by the commission agreement's
    /// pro-rata settlement and passed in; the two amounts must account for the
    /// escrowed total exactly, so no dust can be stranded. The platform fee is
    /// charged only on the artist's share — the client's refund is not taxed.
    ///
    /// CEI: Checks → Effects (status update) → Interactions (transfers).
    pub fn cancel_escrow(
        env: Env,
        commission_id: Bytes,
        config_contract: Address,
        artist_amount: i128,
        client_refund: i128,
    ) -> Result<(), EscrowError> {
        // CHECKS
        let mut r = get_escrow(&env, &commission_id);
        if r.status != CommissionStatus::Locked && r.status != CommissionStatus::Disputed {
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

        let admin: Address = env.invoke_contract(&config_contract, &symbol_short!("get_adm"), soroban_sdk::vec![&env]);
        admin.require_auth();
        let usdc: Address = env.invoke_contract(&config_contract, &symbol_short!("get_usdc"), soroban_sdk::vec![&env]);
        let pw: Address = env.invoke_contract(&config_contract, &symbol_short!("get_pw"), soroban_sdk::vec![&env]);

        let artist = r.artist.clone();
        let client = r.client.clone();

        storage::with_reentrancy_guard(&env, || {
            let (fee, payout) = calculate_fee_split(artist_amount, r.fee_bps)?;

            // EFFECTS
            r.status = CommissionStatus::Cancelled;
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
        if !escrow_exists(&env, &commission_id) { return Err(EscrowError::NotFound); }
        Ok(storage::get_escrow(&env, &commission_id))
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod refund_tests;
#[cfg(test)]
mod dispute_tests;
#[cfg(test)]
mod fee_math_tests;
#[cfg(test)]
mod storage_edge_tests;
#[cfg(test)]
mod cancellation_tests;
#[cfg(test)]
mod integration_tests;
