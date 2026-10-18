//! Dispute Arbiter Smart Contract
//!
//! Autonomous arbitration and dispute settlement for StellarAid escrows.
//! Architecture Decision: [ADR-0004](../../docs/ADRs/0004-dispute-resolution-and-arbitration.md)

#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Bytes, Env, IntoVal, String};

pub mod errors;
pub mod types;

use errors::DisputeError;
use shared::guard;
use types::{DataKey, DisputeRecord, DisputeStatus};

include!("../../semver_types.rs");

#[contract]
pub struct DisputeArbiter;

fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

fn get_admin(env: &Env) -> Result<Address, DisputeError> {
    if !has_admin(env) {
        return Err(DisputeError::NotInitialized);
    }
    Ok(env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap())
}

fn get_escrow_contract(env: &Env) -> Result<Address, DisputeError> {
    if !has_admin(env) {
        return Err(DisputeError::NotInitialized);
    }
    Ok(env
        .storage()
        .instance()
        .get(&DataKey::EscrowContract)
        .unwrap())
}

fn get_config_contract(env: &Env) -> Result<Address, DisputeError> {
    if !has_admin(env) {
        return Err(DisputeError::NotInitialized);
    }
    Ok(env
        .storage()
        .instance()
        .get(&DataKey::ConfigContract)
        .unwrap())
}

fn get_auto_resolve_ledgers(env: &Env) -> Result<u32, DisputeError> {
    if !has_admin(env) {
        return Err(DisputeError::NotInitialized);
    }
    Ok(env
        .storage()
        .instance()
        .get(&DataKey::AutoResolveLedgers)
        .unwrap())
}

fn dispute_exists(env: &Env, commission_id: &Bytes) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::Dispute(commission_id.clone()))
}

fn load_dispute(env: &Env, commission_id: &Bytes) -> Result<DisputeRecord, DisputeError> {
    if !dispute_exists(env, commission_id) {
        return Err(DisputeError::NotFound);
    }
    Ok(env
        .storage()
        .persistent()
        .get(&DataKey::Dispute(commission_id.clone()))
        .unwrap())
}

fn save_dispute(env: &Env, record: &DisputeRecord) {
    env.storage()
        .persistent()
        .set(&DataKey::Dispute(record.commission_id.clone()), record);
}

/// Collapse the shared handover error onto this contract's error enum (#983).
///
/// `DisputeError` predates `shared::admin` and cannot grow new variants here,
/// so the three shared failure modes map onto the two it already has: a missing
/// contract is `NotInitialized`, and every other refusal (wrong caller, no
/// pending transfer, proposing the current admin) is an authorization answer,
/// so `Unauthorized`.
fn map_admin_error(err: shared::admin::AdminError) -> DisputeError {
    match err {
        shared::admin::AdminError::NotInitialized => DisputeError::NotInitialized,
        _ => DisputeError::Unauthorized,
    }
}

#[contractimpl]
impl DisputeArbiter {
    pub fn initialize(
        env: Env,
        admin: Address,
        escrow_contract: Address,
        config_contract: Address,
        auto_resolve_ledgers: u32,
    ) -> Result<(), DisputeError> {
        if has_admin(&env) {
            return Err(DisputeError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::EscrowContract, &escrow_contract);
        env.storage()
            .instance()
            .set(&DataKey::ConfigContract, &config_contract);
        env.storage()
            .instance()
            .set(&DataKey::AutoResolveLedgers, &auto_resolve_ledgers);
        env.events()
            .publish((symbol_short!("init"),), (admin, escrow_contract, config_contract, auto_resolve_ledgers));
        Ok(())
    }

    impl_semver_queries!();

    pub fn open_dispute(
        env: Env,
        commission_id: Bytes,
        initiator: Address,
    ) -> Result<(), DisputeError> {
        // Guard first (#762): `open_dispute` writes a dispute record that the
        // resolve paths read, so a re-entrant call must be refused before it
        // is applied.
        guard::acquire(&env);
        if !has_admin(&env) {
            return Err(DisputeError::NotInitialized);
        }
        initiator.require_auth();
        if dispute_exists(&env, &commission_id) {
            return Err(DisputeError::AlreadyResolved);
        }
        let current_ledger = env.ledger().sequence();
        let auto_resolve_offset = get_auto_resolve_ledgers(&env)?;
        // Checked addition (#760): a wrapped auto-resolve ledger would make a
        // dispute unresolvable.
        let auto_resolve_ledger = current_ledger
            .checked_add(auto_resolve_offset)
            .ok_or(DisputeError::ArithmeticOverflow)?;
        let record = DisputeRecord {
            commission_id: commission_id.clone(),
            opened_ledger: current_ledger,
            auto_resolve_ledger,
            status: DisputeStatus::Open,
            resolution_note: None,
        };
        save_dispute(&env, &record);
        guard::release(&env);
        env.events()
            .publish((symbol_short!("opened"),), (commission_id, initiator, current_ledger, auto_resolve_ledger));
        Ok(())
    }

    pub fn resolve_for_client(
        env: Env,
        commission_id: Bytes,
        note: String,
    ) -> Result<(), DisputeError> {
        // Guard first (#762): the refund below is an external contract call,
        // which is exactly the window a re-entrant call would exploit.
        guard::acquire(&env);
        let admin = get_admin(&env)?;
        admin.require_auth();
        // #765: the note is written to persistent storage, so bound it before
        // the escrow refund rather than after it.
        if note.len() > shared::validation::MAX_MEMO_LEN {
            return Err(DisputeError::NoteTooLong);
        }
        let mut record = load_dispute(&env, &commission_id)?;
        if record.status != DisputeStatus::Open {
            return Err(DisputeError::InvalidStatus);
        }
        let escrow_contract = get_escrow_contract(&env)?;
        let config_contract = get_config_contract(&env)?;
        env.invoke_contract::<()>(
            &escrow_contract,
            &symbol_short!("refund_cl"),
            soroban_sdk::vec![&env, commission_id.clone().into_val(&env), config_contract.into_val(&env)],
        );
        record.status = DisputeStatus::ResolvedForClient;
        record.resolution_note = Some(note.clone());
        save_dispute(&env, &record);
        guard::release(&env);
        env.events()
            .publish((symbol_short!("resolved"),), (commission_id, DisputeStatus::ResolvedForClient, note));
        Ok(())
    }

    pub fn resolve_for_artist(
        env: Env,
        commission_id: Bytes,
        note: String,
    ) -> Result<(), DisputeError> {
        guard::acquire(&env);
        let admin = get_admin(&env)?;
        admin.require_auth();
        // #765: the note is written to persistent storage, so bound it before
        // the escrow release rather than after it.
        if note.len() > shared::validation::MAX_MEMO_LEN {
            return Err(DisputeError::NoteTooLong);
        }
        let mut record = load_dispute(&env, &commission_id)?;
        if record.status != DisputeStatus::Open {
            return Err(DisputeError::InvalidStatus);
        }
        let escrow_contract = get_escrow_contract(&env)?;
        let config_contract = get_config_contract(&env)?;
        env.invoke_contract::<()>(
            &escrow_contract,
            &symbol_short!("rel_pay"),
            soroban_sdk::vec![&env, commission_id.clone().into_val(&env), config_contract.into_val(&env)],
        );
        record.status = DisputeStatus::ResolvedForArtist;
        record.resolution_note = Some(note.clone());
        save_dispute(&env, &record);
        guard::release(&env);
        env.events()
            .publish((symbol_short!("resolved"),), (commission_id, DisputeStatus::ResolvedForArtist, note));
        Ok(())
    }

    pub fn partial_resolve(
        env: Env,
        commission_id: Bytes,
        client_share_bps: u32,
        note: String,
    ) -> Result<(), DisputeError> {
        guard::acquire(&env);
        let admin = get_admin(&env)?;
        admin.require_auth();
        if client_share_bps > 10000 {
            return Err(DisputeError::InvalidShareBps);
        }
        // #765: the note is written to persistent storage, so bound it before
        // the escrow refund and the transfers rather than after them.
        if note.len() > shared::validation::MAX_MEMO_LEN {
            return Err(DisputeError::NoteTooLong);
        }
        let mut record = load_dispute(&env, &commission_id)?;
        if record.status != DisputeStatus::Open {
            return Err(DisputeError::InvalidStatus);
        }
        let escrow_contract = get_escrow_contract(&env)?;
        let config_contract = get_config_contract(&env)?;

        let artist_share_bps = 10000u32
            .checked_sub(client_share_bps)
            .ok_or(DisputeError::InvalidShareBps)?;

        env.invoke_contract::<()>(
            &escrow_contract,
            &symbol_short!("refund_cl"),
            soroban_sdk::vec![&env, commission_id.clone().into_val(&env), config_contract.clone().into_val(&env)],
        );

        let usdc_token: Address = env.invoke_contract(
            &config_contract,
            &symbol_short!("get_usdc"),
            soroban_sdk::vec![&env],
        );
        let escrow_balance: i128 = env.invoke_contract(
            &usdc_token,
            &symbol_short!("balance"),
            soroban_sdk::vec![&env, escrow_contract.clone().into_val(&env)],
        );

        // Checked multiplication (#760): an unchecked product here could wrap a
        // share to a negative amount and drain the escrow on transfer.
        let client_share = escrow_balance
            .checked_mul(client_share_bps as i128)
            .ok_or(DisputeError::ArithmeticOverflow)?
            / 10000;
        let artist_share = escrow_balance
            .checked_mul(artist_share_bps as i128)
            .ok_or(DisputeError::ArithmeticOverflow)?
            / 10000;

        if client_share > 0 {
            env.invoke_contract::<()>(
                &usdc_token,
                &symbol_short!("transfer"),
                soroban_sdk::vec![
                    &env,
                    escrow_contract.clone().into_val(&env),
                    record.commission_id.clone().into_val(&env),
                    client_share.into_val(&env),
                ],
            );
        }
        if artist_share > 0 {
            env.invoke_contract::<()>(
                &usdc_token,
                &symbol_short!("transfer"),
                soroban_sdk::vec![
                    &env,
                    escrow_contract.into_val(&env),
                    record.commission_id.clone().into_val(&env),
                    artist_share.into_val(&env),
                ],
            );
        }

        record.status = DisputeStatus::PartiallyResolved;
        record.resolution_note = Some(note.clone());
        save_dispute(&env, &record);
        guard::release(&env);
        env.events().publish(
            (symbol_short!("resolved"),),
            (commission_id, DisputeStatus::PartiallyResolved, client_share_bps, note),
        );
        Ok(())
    }

    pub fn auto_resolve(env: Env, commission_id: Bytes) -> Result<(), DisputeError> {
        if !has_admin(&env) {
            return Err(DisputeError::NotInitialized);
        }
        let mut record = load_dispute(&env, &commission_id)?;
        if record.status != DisputeStatus::Open {
            return Err(DisputeError::InvalidStatus);
        }
        let current_ledger = env.ledger().sequence();
        if current_ledger < record.auto_resolve_ledger {
            return Err(DisputeError::AutoResolveNotDue);
        }
        let escrow_contract = get_escrow_contract(&env)?;
        let config_contract = get_config_contract(&env)?;
        env.invoke_contract::<()>(
            &escrow_contract,
            &symbol_short!("refund_cl"),
            soroban_sdk::vec![&env, commission_id.clone().into_val(&env), config_contract.into_val(&env)],
        );
        record.status = DisputeStatus::AutoResolved;
        save_dispute(&env, &record);
        env.events()
            .publish((symbol_short!("auto_res"),), (commission_id, current_ledger));
        Ok(())
    }

    pub fn get_dispute(env: Env, commission_id: Bytes) -> Result<DisputeRecord, DisputeError> {
        load_dispute(&env, &commission_id)
    }

    // ── Admin key rotation (closes #983) ─────────────────────────────────
    //
    // Two-step handover, with the state machine defined once in
    // `shared::admin`. The active admin stays under this contract's
    // `DataKey::Admin`; only the pending successor lives in the shared key
    // space. Each entry point checks the caller against the stored admin
    // *before* delegating, because the shared layer cannot read a
    // contract-specific key.

    /// Step 1 — the current admin proposes `new_admin` as its successor.
    pub fn transfer_admin(env: Env, admin: Address, new_admin: Address) -> Result<(), DisputeError> {
        let stored: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(DisputeError::NotInitialized)?;
        if stored != admin {
            return Err(DisputeError::Unauthorized);
        }
        shared::admin::propose(&env, &admin, &new_admin).map_err(map_admin_error)
    }

    /// Step 2 — the proposed successor accepts and becomes the active admin.
    pub fn accept_admin(env: Env, new_admin: Address) -> Result<(), DisputeError> {
        let previous: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(DisputeError::NotInitialized)?;
        shared::admin::accept(&env, &previous, &new_admin).map_err(map_admin_error)?;
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        Ok(())
    }

    /// The current admin withdraws an in-flight proposal; the active admin is
    /// left in place.
    pub fn cancel_admin_transfer(env: Env, admin: Address) -> Result<(), DisputeError> {
        let stored: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(DisputeError::NotInitialized)?;
        if stored != admin {
            return Err(DisputeError::Unauthorized);
        }
        shared::admin::cancel(&env, &admin).map_err(map_admin_error)
    }

    /// The proposed successor awaiting acceptance, if any.
    pub fn get_pending_admin(env: Env) -> Option<Address> {
        shared::admin::pending_admin(&env)
    }

    // ── Health monitoring (#678) and gradual rollout (#684) ──────────────
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
    pub fn get_sla_targets(env: Env) -> shared::health::SlaTargets {
        let _ = env;
        shared::health::sla_targets()
    }
    pub fn set_alert_config(env: Env, admin: Address, config: shared::health::AlertConfig) {
        admin.require_auth();
        shared::health::set_alert_config(&env, config);
    }
    pub fn get_alert_config(env: Env) -> shared::health::AlertConfig {
        shared::health::get_alert_config(&env)
    }
    pub fn detect_anomaly(env: Env) -> bool {
        shared::health::detect_anomaly(&env)
    }
    pub fn report_ok(env: Env, admin: Address) {
        admin.require_auth();
        shared::health::record_ok(&env);
    }
    pub fn report_error(env: Env, admin: Address) {
        admin.require_auth();
        shared::health::record_error(&env);
    }
    pub fn set_feature_flag(env: Env, admin: Address, flag: soroban_sdk::Symbol, enabled: bool) {
        admin.require_auth();
        shared::rollout::set_feature_flag(&env, &flag, enabled);
    }
    pub fn is_feature_enabled(env: Env, flag: soroban_sdk::Symbol) -> bool {
        shared::rollout::is_feature_enabled(&env, &flag)
    }
    /// Lower this contract's feature-flag cap within the shared ceiling (#985).
    ///
    /// `shared::rollout::MAX_FEATURE_FLAGS` is the hard ceiling; this only ever
    /// lowers the effective cap, so future tuning needs no new code. The current
    /// value is returned by [`get_flag_cap`] and in
    /// `get_rollout_state().flag_cap`.
    pub fn set_flag_cap(
        env: Env,
        admin: Address,
        cap: u32,
    ) -> Result<(), shared::rollout::RolloutError> {
        admin.require_auth();
        shared::rollout::set_flag_cap(&env, cap)
    }
    /// The effective feature-flag cap for this contract (#985).
    pub fn get_flag_cap(env: Env) -> u32 {
        shared::rollout::flag_cap(&env)
    }
    pub fn set_canary_deployment(env: Env, admin: Address, canary: Address, stable: Address, canary_bps: u32) {
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
    pub fn trigger_rollback(env: Env, admin: Address) {
        admin.require_auth();
        shared::rollout::trigger_rollback(&env, &admin);
    }
}


#[cfg(test)]
mod test;
