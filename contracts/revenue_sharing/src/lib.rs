#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, token, Address, Bytes, Env, String, Vec};

pub mod errors;
pub mod types;

#[cfg(test)]
mod test;

use errors::RevenueError;
use types::{
    Agreement, AgreementStatus, DataKey, Participant, RevenueEntry, RevenueReport,
    MAX_PARTICIPANTS, TOTAL_BPS,
};

include!("../../semver_types.rs");

#[contract]
pub struct RevenueSharing;

fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

fn require_initialized(env: &Env) -> Result<(), RevenueError> {
    shared::initialization::require_initialized(env, &DataKey::Admin, RevenueError::NotInitialized)
}

fn load_agreement(env: &Env, id: &Bytes) -> Result<Agreement, RevenueError> {
    env.storage()
        .persistent()
        .get(&DataKey::Agreement(id.clone()))
        .ok_or(RevenueError::AgreementNotFound)
}

/// Collapse the shared handover error onto `RevenueError` (#983).
///
/// `RevenueError` cannot grow new variants here, so a missing contract maps to
/// `NotInitialized` and every other refusal (wrong caller, no pending transfer,
/// proposing the current admin) is `Unauthorized`.
fn map_admin_error(err: shared::admin::AdminError) -> RevenueError {
    match err {
        shared::admin::AdminError::NotInitialized => RevenueError::NotInitialized,
        _ => RevenueError::Unauthorized,
    }
}

fn save_agreement(env: &Env, agreement: &Agreement) {
    env.storage()
        .persistent()
        .set(&DataKey::Agreement(agreement.id.clone()), agreement);
}

fn load_splits(env: &Env, id: &Bytes) -> Vec<Participant> {
    env.storage()
        .persistent()
        .get(&DataKey::Splits(id.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

/// Splits must cover exactly 100% and name each account once, otherwise a
/// payout would either leak value or pay the same account twice.
fn validate_splits(participants: &Vec<Participant>) -> Result<(), RevenueError> {
    if participants.is_empty() {
        return Err(RevenueError::EmptySplit);
    }
    if participants.len() > MAX_PARTICIPANTS {
        return Err(RevenueError::TooManyParticipants);
    }
    let mut total: u32 = 0;
    for (i, participant) in participants.iter().enumerate() {
        if participant.share_bps == 0 {
            return Err(RevenueError::InvalidSplitTotal);
        }
        total = total
            .checked_add(participant.share_bps)
            .ok_or(RevenueError::ArithmeticOverflow)?;
        for other in participants.iter().skip(i + 1) {
            if other.account == participant.account {
                return Err(RevenueError::DuplicateParticipant);
            }
        }
    }
    if total != TOTAL_BPS {
        return Err(RevenueError::InvalidSplitTotal);
    }
    Ok(())
}

fn add_earnings(env: &Env, id: &Bytes, account: &Address, amount: i128) {
    let key = DataKey::Earnings(id.clone(), account.clone());
    let current: i128 = env.storage().persistent().get(&key).unwrap_or(0);
    env.storage().persistent().set(&key, &(current + amount));
}

fn push_history(env: &Env, id: &Bytes, entry: RevenueEntry) {
    let key = DataKey::History(id.clone());
    let mut history: Vec<RevenueEntry> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));
    let limit: u32 = env
        .storage()
        .instance()
        .get(&DataKey::HistoryLimit)
        .unwrap_or(0);
    while history.len() >= limit {
        history.pop_front();
    }
    history.push_back(entry);
    env.storage().persistent().set(&key, &history);
}

#[contractimpl]
impl RevenueSharing {
    pub fn initialize(env: Env, admin: Address, history_limit: u32) -> Result<(), RevenueError> {
        if has_admin(&env) {
            return Err(RevenueError::AlreadyInitialized);
        }
        if history_limit == 0 {
            return Err(RevenueError::InvalidAmount);
        }
        // Clamp the retained history to the shared page ceiling (#984).
        let history_limit = shared::pagination::clamp_limit(history_limit);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::HistoryLimit, &history_limit);
        env.events()
            .publish((symbol_short!("init"),), (admin, history_limit));
        Ok(())
    }

    impl_semver_queries!();

    pub fn create_agreement(
        env: Env,
        id: Bytes,
        owner: Address,
        token: Address,
        participants: Vec<Participant>,
    ) -> Result<(), RevenueError> {
        require_initialized(&env)?;
        owner.require_auth();
        if env
            .storage()
            .persistent()
            .has(&DataKey::Agreement(id.clone()))
        {
            return Err(RevenueError::AgreementExists);
        }
        validate_splits(&participants)?;
        let ledger = env.ledger().sequence();
        let agreement = Agreement {
            id: id.clone(),
            owner: owner.clone(),
            token,
            status: AgreementStatus::Active,
            terms_version: 1,
            total_revenue: 0,
            total_distributed: 0,
            entry_count: 0,
            created_ledger: ledger,
            updated_ledger: ledger,
        };
        save_agreement(&env, &agreement);
        env.storage()
            .persistent()
            .set(&DataKey::Splits(id.clone()), &participants);
        env.events()
            .publish((symbol_short!("created"),), (id, owner));
        Ok(())
    }

    /// Replace the split terms. Revenue already booked keeps its attribution;
    /// only future distributions follow the new terms.
    pub fn update_splits(
        env: Env,
        id: Bytes,
        participants: Vec<Participant>,
    ) -> Result<u32, RevenueError> {
        require_initialized(&env)?;
        let mut agreement = load_agreement(&env, &id)?;
        agreement.owner.require_auth();
        if agreement.status == AgreementStatus::Terminated {
            return Err(RevenueError::AgreementNotActive);
        }
        validate_splits(&participants)?;
        agreement.terms_version += 1;
        agreement.updated_ledger = env.ledger().sequence();
        save_agreement(&env, &agreement);
        env.storage()
            .persistent()
            .set(&DataKey::Splits(id.clone()), &participants);
        env.events()
            .publish((symbol_short!("terms"),), (id, agreement.terms_version));
        Ok(agreement.terms_version)
    }

    pub fn set_status(env: Env, id: Bytes, status: AgreementStatus) -> Result<(), RevenueError> {
        require_initialized(&env)?;
        let mut agreement = load_agreement(&env, &id)?;
        agreement.owner.require_auth();
        if agreement.status == AgreementStatus::Terminated {
            return Err(RevenueError::AgreementNotActive);
        }
        agreement.status = status;
        agreement.updated_ledger = env.ledger().sequence();
        save_agreement(&env, &agreement);
        env.events()
            .publish((symbol_short!("status"),), (id, status));
        Ok(())
    }

    /// Book revenue and pay every participant in the same call. Each share is
    /// floored to whole token units and the rounding dust is added to the first
    /// participant, so the distributed total always equals the gross amount.
    pub fn record_revenue(
        env: Env,
        id: Bytes,
        source: Address,
        amount: i128,
        memo: String,
    ) -> Result<i128, RevenueError> {
        require_initialized(&env)?;
        source.require_auth();
        if amount <= 0 {
            return Err(RevenueError::InvalidAmount);
        }
        let mut agreement = load_agreement(&env, &id)?;
        if agreement.status != AgreementStatus::Active {
            return Err(RevenueError::AgreementNotActive);
        }
        let participants = load_splits(&env, &id);
        validate_splits(&participants)?;

        let mut payouts: Vec<i128> = Vec::new(&env);
        let mut allocated: i128 = 0;
        for participant in participants.iter() {
            let payout = amount
                .checked_mul(participant.share_bps as i128)
                .ok_or(RevenueError::ArithmeticOverflow)?
                / TOTAL_BPS as i128;
            allocated += payout;
            payouts.push_back(payout);
        }
        let dust = amount - allocated;
        if dust > 0 {
            payouts.set(0, payouts.get(0).unwrap() + dust);
        }

        // Effects first: attribution and totals are committed before any token
        // moves, per the checks-effects-interactions pattern.
        let ledger = env.ledger().sequence();
        for (i, participant) in participants.iter().enumerate() {
            add_earnings(
                &env,
                &id,
                &participant.account,
                payouts.get(i as u32).unwrap(),
            );
        }
        agreement.total_revenue = agreement
            .total_revenue
            .checked_add(amount)
            .ok_or(RevenueError::ArithmeticOverflow)?;
        agreement.total_distributed = agreement
            .total_distributed
            .checked_add(amount)
            .ok_or(RevenueError::ArithmeticOverflow)?;
        agreement.entry_count += 1;
        agreement.updated_ledger = ledger;
        save_agreement(&env, &agreement);
        push_history(
            &env,
            &id,
            RevenueEntry {
                sequence: agreement.entry_count,
                source: source.clone(),
                gross: amount,
                distributed: amount,
                terms_version: agreement.terms_version,
                ledger,
                memo,
            },
        );

        let token_client = token::Client::new(&env, &agreement.token);
        for (i, participant) in participants.iter().enumerate() {
            let payout = payouts.get(i as u32).unwrap();
            if payout > 0 {
                token_client.transfer(&source, &participant.account, &payout);
            }
        }

        env.events()
            .publish((symbol_short!("revenue"),), (id, source, amount));
        Ok(amount)
    }

    pub fn get_agreement(env: Env, id: Bytes) -> Result<Agreement, RevenueError> {
        load_agreement(&env, &id)
    }

    pub fn get_splits(env: Env, id: Bytes) -> Vec<Participant> {
        load_splits(&env, &id)
    }

    /// Lifetime amount attributed to `account` under this agreement.
    pub fn get_earnings(env: Env, id: Bytes, account: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Earnings(id, account))
            .unwrap_or(0)
    }

    pub fn get_history(env: Env, id: Bytes) -> Vec<RevenueEntry> {
        env.storage()
            .persistent()
            .get(&DataKey::History(id))
            .unwrap_or_else(|| Vec::new(&env))
    }

    // ── Bounded reads (closes #984) ──────────────────────────────────────
    //
    // `push_history` retains at most `clamp_limit(history_limit)` entries (see
    // `initialize`), so `get_history` is already bounded at
    // `shared::pagination::MAX_LIMIT`. This variant adds an offset cursor and
    // `PageInfo` so a client can page the retained window and tell a truncated
    // page from a complete one; the caller's `limit` is clamped by
    // `collect_window`.

    /// One page of `id`'s retained revenue history, plus its [`PageInfo`].
    ///
    /// [`PageInfo`]: shared::pagination::PageInfo
    pub fn get_history_page(
        env: Env,
        id: Bytes,
        start: u32,
        limit: u32,
    ) -> (Vec<RevenueEntry>, shared::pagination::PageInfo) {
        let all: Vec<RevenueEntry> = env
            .storage()
            .persistent()
            .get(&DataKey::History(id))
            .unwrap_or_else(|| Vec::new(&env));
        let total = all.len();
        shared::pagination::collect_window(&env, total, start, limit, |i| all.get(i))
    }

    // ── Admin key rotation (closes #983) ─────────────────────────────────
    //
    // Two-step handover delegated to `shared::admin`; the active admin stays
    // under `DataKey::Admin`, only the pending successor is shared.

    /// Step 1 — the current admin proposes `new_admin` as its successor.
    pub fn transfer_admin(env: Env, admin: Address, new_admin: Address) -> Result<(), RevenueError> {
        let stored: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(RevenueError::NotInitialized)?;
        if stored != admin {
            return Err(RevenueError::Unauthorized);
        }
        shared::admin::propose(&env, &admin, &new_admin).map_err(map_admin_error)
    }

    /// Step 2 — the proposed successor accepts and becomes the active admin.
    pub fn accept_admin(env: Env, new_admin: Address) -> Result<(), RevenueError> {
        let previous: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(RevenueError::NotInitialized)?;
        shared::admin::accept(&env, &previous, &new_admin).map_err(map_admin_error)?;
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        Ok(())
    }

    /// The current admin withdraws an in-flight proposal.
    pub fn cancel_admin_transfer(env: Env, admin: Address) -> Result<(), RevenueError> {
        let stored: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(RevenueError::NotInitialized)?;
        if stored != admin {
            return Err(RevenueError::Unauthorized);
        }
        shared::admin::cancel(&env, &admin).map_err(map_admin_error)
    }

    /// The proposed successor awaiting acceptance, if any.
    pub fn get_pending_admin(env: Env) -> Option<Address> {
        shared::admin::pending_admin(&env)
    }

    pub fn get_report(env: Env, id: Bytes) -> Result<RevenueReport, RevenueError> {
        let agreement = load_agreement(&env, &id)?;
        Ok(RevenueReport {
            total_revenue: agreement.total_revenue,
            total_distributed: agreement.total_distributed,
            entry_count: agreement.entry_count,
            terms_version: agreement.terms_version,
            status: agreement.status,
        })
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

