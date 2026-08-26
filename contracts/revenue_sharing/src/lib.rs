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

#[contract]
pub struct RevenueSharing;

fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

fn require_initialized(env: &Env) -> Result<(), RevenueError> {
    if has_admin(env) {
        Ok(())
    } else {
        Err(RevenueError::NotInitialized)
    }
}

fn load_agreement(env: &Env, id: &Bytes) -> Result<Agreement, RevenueError> {
    env.storage()
        .persistent()
        .get(&DataKey::Agreement(id.clone()))
        .ok_or(RevenueError::AgreementNotFound)
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
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::HistoryLimit, &history_limit);
        env.events()
            .publish((symbol_short!("init"),), (admin, history_limit));
        Ok(())
    }

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
}
