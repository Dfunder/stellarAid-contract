//! CommissionAgreement contract — core agreement lifecycle functions.
//!
//! Implements:
//! - `create_agreement`        (closes #457, closes #458, closes #591, closes #592)
//! - `accept_agreement`        (closes #459)
//! - `reject_agreement`        (closes #459)
//! - `propose_milestone`       (closes #460)
//! - `set_cancellation_policy` (closes #605)
//! - `cancel_agreement`        (closes #605)
//! - `register_agency`         (closes #609)
//! - `add_artist` / `remove_artist` / `set_artist_split` / `distribute_batch` (closes #609)
//! - `request_revision`        (closes #600)
//! - `accept_revision`         (closes #600)
//! - `reject_revision`         (closes #600)
//! - `expire_revision`         (closes #600)
//! - `set_revision_limit`      (closes #600)
//! - `get_revisions`           (closes #600)
//! - `add_team_member`         (closes #603)
//! - `remove_team_member`      (closes #603)
//! - `update_team_member_role` (closes #603)
//! - `set_payment_split`       (closes #603)

#![no_std]

#[cfg(all(test, feature = "legacy_tests"))]
mod test;

#[cfg(all(test, feature = "legacy_tests"))]
mod milestone_flow;

#[cfg(all(test, feature = "legacy_tests"))]
mod multiple_escrows;

#[cfg(all(test, feature = "legacy_tests"))]
mod dispute_resolution;

pub mod agency;
pub mod cancellation;
pub mod errors;
pub mod types;

#[cfg(test)]
mod agency_tests;

#[cfg(test)]
mod cancellation_tests;

#[cfg(test)]
mod revision_tests;

#[cfg(test)]
mod team_tests;

use soroban_sdk::{contract, contractimpl, symbol_short, token, Address, Bytes, Env, String, Vec};
use agency::{AgencyAnalytics, AgencyProfile, BatchPayment, RosterEntry};
use cancellation::{CancellationPolicy, CancellationQuote, CancellationReason, CancellationRecord};
use errors::AgreementError;
use types::{
    AgreementRecord, AgreementStatus, DataKey, MilestoneRecord, MilestoneStatus,
    PaymentSplitEntry, RevisionConfig, RevisionProposer, RevisionRecord, RevisionStatus,
    TeamMember, TeamRole,
};

/// Cap on the retained cancellation history.
const CANCELLATION_HISTORY_LIMIT: u32 = 50;

// ── Input length limits (closes #591) ────────────────────────────────────────
const MAX_TITLE_LEN: u32 = 128;
const MAX_MILESTONE_TITLE_LEN: u32 = 128;
const MAX_REASON_LEN: u32 = 512;
const MAX_ID_LEN: u32 = 64;

// ── Deadline upper bound (closes #592) ───────────────────────────────────────
const MAX_DEADLINE_OFFSET_LEDGERS: u32 = 12_614_400;

fn load_agreement(env: &Env, commission_id: &Bytes) -> Result<AgreementRecord, AgreementError> {
    env.storage()
        .persistent()
        .get(&DataKey::Agreement(commission_id.clone()))
        .ok_or(AgreementError::NotFound)
}

fn approved_total(env: &Env, commission_id: &Bytes) -> i128 {
    let milestones: Vec<MilestoneRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::MilestonesForAgreement(commission_id.clone()))
        .unwrap_or_else(|| Vec::new(env));
    milestones
        .iter()
        .filter(|m| m.status == MilestoneStatus::Approved)
        .map(|m| m.amount_usdc)
        .sum()
}

fn load_policy(env: &Env, commission_id: &Bytes) -> CancellationPolicy {
    env.storage()
        .persistent()
        .get(&DataKey::CancellationPolicy(commission_id.clone()))
        .unwrap_or_else(cancellation::default_policy)
}

fn load_agency(env: &Env, agency: &Address) -> Result<AgencyProfile, AgreementError> {
    env.storage()
        .persistent()
        .get(&DataKey::Agency(agency.clone()))
        .ok_or(AgreementError::AgencyNotFound)
}

fn load_roster_entry(
    env: &Env,
    agency: &Address,
    artist: &Address,
) -> Result<RosterEntry, AgreementError> {
    env.storage()
        .persistent()
        .get(&DataKey::RosterEntry(agency.clone(), artist.clone()))
        .ok_or(AgreementError::ArtistNotOnRoster)
}

fn load_analytics(env: &Env, agency: &Address) -> AgencyAnalytics {
    env.storage()
        .persistent()
        .get(&DataKey::AgencyAnalytics(agency.clone()))
        .unwrap_or_default()
}

fn save_analytics(env: &Env, agency: &Address, analytics: &AgencyAnalytics) {
    env.storage()
        .persistent()
        .set(&DataKey::AgencyAnalytics(agency.clone()), analytics);
}

fn attribute_commission(env: &Env, artist: &Address, budget_usdc: i128) {
    let agency: Option<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::ArtistAgency(artist.clone()));
    let Some(agency) = agency else { return };
    if let Ok(mut entry) = load_roster_entry(env, &agency, artist) {
        entry.commissions += 1;
        env.storage()
            .persistent()
            .set(&DataKey::RosterEntry(agency.clone(), artist.clone()), &entry);
    }
    let mut analytics = load_analytics(env, &agency);
    analytics.commissions += 1;
    analytics.commission_budget += budget_usdc;
    save_analytics(env, &agency, &analytics);
}

#[contract]
pub struct CommissionAgreementContract;

#[contractimpl]
impl CommissionAgreementContract {
    // ── Agreement lifecycle ─────────────────────────────────────────────────

    /// Create a new commission agreement.
    ///
    /// Closes #457, #458, #591 (input validation), #592 (deadline upper bound).
    pub fn create_agreement(
        env: Env,
        commission_id: Bytes,
        client: Address,
        artist: Address,
        title: String,
        budget_usdc: i128,
        deadline_ledger: u32,
    ) -> Result<(), AgreementError> {
        client.require_auth();

        if commission_id.len() > MAX_ID_LEN {
            return Err(AgreementError::InputTooLong);
        }
        if title.len() > MAX_TITLE_LEN {
            return Err(AgreementError::InputTooLong);
        }
        if budget_usdc <= 0 {
            return Err(AgreementError::InvalidAmount);
        }
        if deadline_ledger <= env.ledger().sequence() {
            return Err(AgreementError::DeadlineInPast);
        }
        let max_deadline = env.ledger().sequence()
            .checked_add(MAX_DEADLINE_OFFSET_LEDGERS)
            .ok_or(AgreementError::ArithmeticOverflow)?;
        if deadline_ledger > max_deadline {
            return Err(AgreementError::DeadlineTooFar);
        }
        if env.storage().persistent().has(&DataKey::Agreement(commission_id.clone())) {
            return Err(AgreementError::AlreadyExists);
        }

        let record = AgreementRecord {
            commission_id: commission_id.clone(),
            client: client.clone(),
            artist: artist.clone(),
            title,
            budget_usdc,
            deadline_ledger,
            status: AgreementStatus::Pending,
            created_ledger: env.ledger().sequence(),
        };

        env.storage().persistent().set(&DataKey::Agreement(commission_id.clone()), &record);
        env.storage().persistent().set(
            &DataKey::MilestonesForAgreement(commission_id.clone()),
            &Vec::<MilestoneRecord>::new(&env),
        );

        attribute_commission(&env, &artist, budget_usdc);

        env.events().publish(
            (symbol_short!("agr_new"),),
            (commission_id, client, artist, budget_usdc),
        );
        Ok(())
    }

    /// Accept a pending agreement (artist auth). Closes #459.
    pub fn accept_agreement(env: Env, commission_id: Bytes) -> Result<(), AgreementError> {
        let mut record = load_agreement(&env, &commission_id)?;
        record.artist.require_auth();

        if record.status != AgreementStatus::Pending {
            return Err(AgreementError::InvalidStatus);
        }

        record.status = AgreementStatus::Active;
        env.storage().persistent().set(&DataKey::Agreement(commission_id.clone()), &record);

        env.events().publish((symbol_short!("agr_ok"),), (commission_id,));
        Ok(())
    }

    /// Reject a pending agreement (artist auth). Closes #459, #591.
    pub fn reject_agreement(env: Env, commission_id: Bytes, reason: String) -> Result<(), AgreementError> {
        if reason.len() > MAX_REASON_LEN {
            return Err(AgreementError::InputTooLong);
        }

        let mut record = load_agreement(&env, &commission_id)?;
        record.artist.require_auth();

        if record.status != AgreementStatus::Pending {
            return Err(AgreementError::InvalidStatus);
        }

        record.status = AgreementStatus::Cancelled;
        env.storage().persistent().set(&DataKey::Agreement(commission_id.clone()), &record);

        env.events().publish((symbol_short!("agr_rej"),), (commission_id, reason));
        Ok(())
    }

    /// Propose a milestone on an active agreement (artist auth). Closes #460, #591.
    pub fn propose_milestone(
        env: Env,
        commission_id: Bytes,
        milestone_id: Bytes,
        title: String,
        amount_usdc: i128,
    ) -> Result<(), AgreementError> {
        if milestone_id.len() > MAX_ID_LEN {
            return Err(AgreementError::InputTooLong);
        }
        if title.len() > MAX_MILESTONE_TITLE_LEN {
            return Err(AgreementError::InputTooLong);
        }

        let record = load_agreement(&env, &commission_id)?;
        record.artist.require_auth();

        if record.status != AgreementStatus::Active {
            return Err(AgreementError::InvalidStatus);
        }
        if amount_usdc <= 0 {
            return Err(AgreementError::InvalidAmount);
        }

        let milestones: Vec<MilestoneRecord> = env.storage().persistent()
            .get(&DataKey::MilestonesForAgreement(commission_id.clone()))
            .unwrap_or(Vec::new(&env));

        let total: i128 = milestones.iter().map(|m| m.amount_usdc).sum();
        if total + amount_usdc > record.budget_usdc {
            return Err(AgreementError::MilestoneBudgetExceeded);
        }

        let milestone = MilestoneRecord {
            milestone_id: milestone_id.clone(),
            commission_id: commission_id.clone(),
            title,
            amount_usdc,
            status: MilestoneStatus::Pending,
        };

        env.storage().persistent().set(
            &DataKey::Milestone(commission_id.clone(), milestone_id.clone()),
            &milestone,
        );
        let mut updated = milestones;
        updated.push_back(milestone);
        env.storage().persistent().set(
            &DataKey::MilestonesForAgreement(commission_id.clone()),
            &updated,
        );

        env.events().publish(
            (symbol_short!("ms_new"),),
            (commission_id, milestone_id, amount_usdc),
        );
        Ok(())
    }

    /// Approve a milestone (client auth). Closes #589 (milestone lock).
    pub fn approve_milestone(env: Env, commission_id: Bytes, milestone_id: Bytes) -> Result<(), AgreementError> {
        let mut record = load_agreement(&env, &commission_id)?;
        record.client.require_auth();

        if record.status != AgreementStatus::Active {
            return Err(AgreementError::InvalidStatus);
        }

        let lock_key = DataKey::MilestoneLock(commission_id.clone(), milestone_id.clone());
        if env.storage().persistent().has(&lock_key) {
            return Err(AgreementError::MilestoneLocked);
        }
        env.storage().persistent().set(&lock_key, &true);

        let mut milestone: MilestoneRecord = env.storage().persistent()
            .get(&DataKey::Milestone(commission_id.clone(), milestone_id.clone()))
            .ok_or(AgreementError::NotFound)?;

        if milestone.status != MilestoneStatus::Pending {
            env.storage().persistent().remove(&lock_key);
            return Err(AgreementError::InvalidStatus);
        }

        milestone.status = MilestoneStatus::Approved;
        env.storage().persistent().set(
            &DataKey::Milestone(commission_id.clone(), milestone_id.clone()),
            &milestone,
        );

        let milestones: Vec<MilestoneRecord> = env.storage().persistent()
            .get(&DataKey::MilestonesForAgreement(commission_id.clone()))
            .unwrap_or(Vec::new(&env));
        let mut updated = Vec::new(&env);
        for m in milestones.iter() {
            if m.milestone_id == milestone_id {
                updated.push_back(milestone.clone());
            } else {
                updated.push_back(m);
            }
        }
        env.storage().persistent().set(
            &DataKey::MilestonesForAgreement(commission_id.clone()),
            &updated,
        );

        let all_approved = !updated.is_empty()
            && updated.iter().all(|m| m.status == MilestoneStatus::Approved);
        if all_approved {
            record.status = AgreementStatus::Completed;
            env.storage().persistent().set(&DataKey::Agreement(commission_id.clone()), &record);
        }

        env.storage().persistent().remove(&lock_key);
        env.events().publish((symbol_short!("ms_aprvd"),), (commission_id, milestone_id));
        Ok(())
    }

    pub fn get_agreement(env: Env, commission_id: Bytes) -> Result<AgreementRecord, AgreementError> {
        load_agreement(&env, &commission_id)
    }

    pub fn get_milestones(env: Env, commission_id: Bytes) -> Result<Vec<MilestoneRecord>, AgreementError> {
        if !env.storage().persistent().has(&DataKey::Agreement(commission_id.clone())) {
            return Err(AgreementError::NotFound);
        }
        Ok(env.storage().persistent()
            .get(&DataKey::MilestonesForAgreement(commission_id))
            .unwrap_or(Vec::new(&env)))
    }

    // ── Cancellation with pro-rata refunds (closes #605) ───────────────────

    pub fn set_cancellation_policy(
        env: Env,
        commission_id: Bytes,
        policy: CancellationPolicy,
    ) -> Result<(), AgreementError> {
        let record = load_agreement(&env, &commission_id)?;
        record.client.require_auth();

        if record.status != AgreementStatus::Pending {
            return Err(AgreementError::InvalidStatus);
        }
        cancellation::validate_policy(&policy)?;

        env.storage()
            .persistent()
            .set(&DataKey::CancellationPolicy(commission_id.clone()), &policy);
        env.events().publish(
            (symbol_short!("canc_pol"),),
            (commission_id, policy.penalty_bps, policy.grace_ledgers),
        );
        Ok(())
    }

    pub fn get_cancellation_policy(env: Env, commission_id: Bytes) -> CancellationPolicy {
        load_policy(&env, &commission_id)
    }

    pub fn quote_cancellation(
        env: Env,
        commission_id: Bytes,
        reason: CancellationReason,
    ) -> Result<CancellationQuote, AgreementError> {
        let record = load_agreement(&env, &commission_id)?;
        let policy = load_policy(&env, &commission_id);
        cancellation::settle(
            record.budget_usdc,
            approved_total(&env, &commission_id),
            reason,
            &policy,
            Self::in_grace(&env, &record, &policy),
        )
    }

    pub fn cancel_agreement(
        env: Env,
        commission_id: Bytes,
        initiator: Address,
        reason: CancellationReason,
    ) -> Result<CancellationRecord, AgreementError> {
        initiator.require_auth();

        let mut record = load_agreement(&env, &commission_id)?;
        if initiator != record.client && initiator != record.artist {
            return Err(AgreementError::Unauthorized);
        }
        if record.status == AgreementStatus::Cancelled {
            return Err(AgreementError::AlreadyCancelled);
        }
        if record.status != AgreementStatus::Pending && record.status != AgreementStatus::Active {
            return Err(AgreementError::NotCancellable);
        }

        let policy = load_policy(&env, &commission_id);
        let quote = cancellation::settle(
            record.budget_usdc,
            approved_total(&env, &commission_id),
            reason,
            &policy,
            Self::in_grace(&env, &record, &policy),
        )?;

        record.status = AgreementStatus::Cancelled;
        env.storage()
            .persistent()
            .set(&DataKey::Agreement(commission_id.clone()), &record);

        let cancellation_record = CancellationRecord {
            commission_id: commission_id.clone(),
            initiator: initiator.clone(),
            reason,
            budget_usdc: record.budget_usdc,
            completion_bps: quote.completion_bps,
            penalty: quote.penalty,
            penalised: quote.penalised,
            artist_amount: quote.artist_amount,
            client_refund: quote.client_refund,
            ledger: env.ledger().sequence(),
        };
        env.storage().persistent().set(
            &DataKey::Cancellation(commission_id.clone()),
            &cancellation_record,
        );

        let mut history: Vec<CancellationRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::CancellationHistory)
            .unwrap_or_else(|| Vec::new(&env));
        while history.len() >= CANCELLATION_HISTORY_LIMIT {
            history.pop_front();
        }
        history.push_back(cancellation_record.clone());
        env.storage()
            .persistent()
            .set(&DataKey::CancellationHistory, &history);

        env.events().publish(
            (symbol_short!("agr_canc"),),
            (commission_id, initiator, reason, quote.completion_bps, quote.artist_amount, quote.client_refund),
        );
        Ok(cancellation_record)
    }

    pub fn get_cancellation(env: Env, commission_id: Bytes) -> Result<CancellationRecord, AgreementError> {
        env.storage()
            .persistent()
            .get(&DataKey::Cancellation(commission_id))
            .ok_or(AgreementError::NotFound)
    }

    pub fn get_cancellation_history(env: Env) -> Vec<CancellationRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::CancellationHistory)
            .unwrap_or_else(|| Vec::new(&env))
    }

    // ── Agency support (closes #609) ───────────────────────────────────────

    pub fn register_agency(
        env: Env,
        agency: Address,
        name: String,
        default_split_bps: u32,
    ) -> Result<(), AgreementError> {
        agency.require_auth();
        if env.storage().persistent().has(&DataKey::Agency(agency.clone())) {
            return Err(AgreementError::AgencyExists);
        }
        agency::validate_split_bps(default_split_bps)?;

        let profile = AgencyProfile {
            agency: agency.clone(),
            name,
            default_split_bps,
            artist_count: 0,
            created_ledger: env.ledger().sequence(),
        };
        env.storage().persistent().set(&DataKey::Agency(agency.clone()), &profile);
        env.storage().persistent().set(
            &DataKey::Roster(agency.clone()),
            &Vec::<Address>::new(&env),
        );

        env.events().publish((symbol_short!("agy_new"),), (agency, default_split_bps));
        Ok(())
    }

    pub fn add_artist(env: Env, agency: Address, artist: Address, split_bps: u32) -> Result<(), AgreementError> {
        let mut profile = load_agency(&env, &agency)?;
        agency.require_auth();
        agency::validate_split_bps(split_bps)?;

        if env.storage().persistent().has(&DataKey::ArtistAgency(artist.clone())) {
            return Err(AgreementError::ArtistAlreadyRepresented);
        }

        let entry = RosterEntry {
            agency: agency.clone(),
            artist: artist.clone(),
            split_bps,
            joined_ledger: env.ledger().sequence(),
            commissions: 0,
            gross_distributed: 0,
            agency_revenue: 0,
            artist_payouts: 0,
        };
        env.storage().persistent().set(
            &DataKey::RosterEntry(agency.clone(), artist.clone()),
            &entry,
        );
        env.storage().persistent().set(&DataKey::ArtistAgency(artist.clone()), &agency);

        let mut roster: Vec<Address> = env.storage().persistent()
            .get(&DataKey::Roster(agency.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        roster.push_back(artist.clone());
        env.storage().persistent().set(&DataKey::Roster(agency.clone()), &roster);

        profile.artist_count += 1;
        env.storage().persistent().set(&DataKey::Agency(agency.clone()), &profile);
        let mut analytics = load_analytics(&env, &agency);
        analytics.artist_count = profile.artist_count;
        save_analytics(&env, &agency, &analytics);

        env.events().publish((symbol_short!("agy_add"),), (agency, artist, split_bps));
        Ok(())
    }

    pub fn remove_artist(env: Env, agency: Address, artist: Address) -> Result<(), AgreementError> {
        let mut profile = load_agency(&env, &agency)?;
        agency.require_auth();
        load_roster_entry(&env, &agency, &artist)?;

        env.storage().persistent().remove(&DataKey::ArtistAgency(artist.clone()));

        let roster: Vec<Address> = env.storage().persistent()
            .get(&DataKey::Roster(agency.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        let mut remaining = Vec::new(&env);
        for member in roster.iter() {
            if member != artist {
                remaining.push_back(member);
            }
        }
        env.storage().persistent().set(&DataKey::Roster(agency.clone()), &remaining);

        profile.artist_count = remaining.len();
        env.storage().persistent().set(&DataKey::Agency(agency.clone()), &profile);
        let mut analytics = load_analytics(&env, &agency);
        analytics.artist_count = profile.artist_count;
        save_analytics(&env, &agency, &analytics);

        env.events().publish((symbol_short!("agy_rm"),), (agency, artist));
        Ok(())
    }

    pub fn set_artist_split(env: Env, agency: Address, artist: Address, split_bps: u32) -> Result<(), AgreementError> {
        load_agency(&env, &agency)?;
        agency.require_auth();
        agency::validate_split_bps(split_bps)?;

        let mut entry = load_roster_entry(&env, &agency, &artist)?;
        entry.split_bps = split_bps;
        env.storage().persistent().set(
            &DataKey::RosterEntry(agency.clone(), artist.clone()),
            &entry,
        );

        env.events().publish((symbol_short!("agy_splt"),), (agency, artist, split_bps));
        Ok(())
    }

    pub fn distribute_batch(
        env: Env,
        agency: Address,
        token_address: Address,
        payments: Vec<BatchPayment>,
    ) -> Result<i128, AgreementError> {
        load_agency(&env, &agency)?;
        agency.require_auth();

        if payments.is_empty() {
            return Err(AgreementError::EmptyBatch);
        }
        if payments.len() > agency::MAX_BATCH {
            return Err(AgreementError::InvalidAmount);
        }

        let mut analytics = load_analytics(&env, &agency);
        let mut total_gross: i128 = 0;
        let mut nets: Vec<i128> = Vec::new(&env);

        for payment in payments.iter() {
            let mut entry = load_roster_entry(&env, &agency, &payment.artist)?;
            let (agency_cut, artist_net) = agency::split_payment(payment.gross_usdc, entry.split_bps)?;

            entry.gross_distributed += payment.gross_usdc;
            entry.agency_revenue += agency_cut;
            entry.artist_payouts += artist_net;
            env.storage().persistent().set(
                &DataKey::RosterEntry(agency.clone(), payment.artist.clone()),
                &entry,
            );

            total_gross = total_gross
                .checked_add(payment.gross_usdc)
                .ok_or(AgreementError::ArithmeticOverflow)?;
            analytics.agency_revenue += agency_cut;
            analytics.artist_payouts += artist_net;
            nets.push_back(artist_net);
        }

        analytics.batches += 1;
        analytics.gross_distributed += total_gross;
        save_analytics(&env, &agency, &analytics);

        let token_client = token::Client::new(&env, &token_address);
        for (i, payment) in payments.iter().enumerate() {
            let net = nets.get(i as u32).unwrap();
            if net > 0 {
                token_client.transfer(&agency, &payment.artist, &net);
            }
        }

        env.events().publish(
            (symbol_short!("agy_btch"),),
            (agency, payments.len(), total_gross),
        );
        Ok(total_gross)
    }

    pub fn get_agency(env: Env, agency: Address) -> Result<AgencyProfile, AgreementError> {
        load_agency(&env, &agency)
    }

    pub fn get_roster(env: Env, agency: Address) -> Vec<Address> {
        env.storage().persistent()
            .get(&DataKey::Roster(agency))
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_roster_entry(env: Env, agency: Address, artist: Address) -> Result<RosterEntry, AgreementError> {
        load_roster_entry(&env, &agency, &artist)
    }

    pub fn get_artist_agency(env: Env, artist: Address) -> Option<Address> {
        env.storage().persistent().get(&DataKey::ArtistAgency(artist))
    }

    pub fn get_agency_analytics(env: Env, agency: Address) -> AgencyAnalytics {
        load_analytics(&env, &agency)
    }

    // ── Revision System — closes #600 ──────────────────────────────────────

    /// Configure the maximum number of revisions for an agreement.
    ///
    /// `max_revisions = 0` means unlimited. Closes #600.
    pub fn set_revision_limit(
        env: Env,
        commission_id: Bytes,
        caller: Address,
        max_revisions: u32,
    ) -> Result<(), AgreementError> {
        caller.require_auth();

        let record = load_agreement(&env, &commission_id)?;
        if caller != record.client && caller != record.artist {
            return Err(AgreementError::Unauthorized);
        }
        if record.status != AgreementStatus::Active {
            return Err(AgreementError::InvalidStatus);
        }

        let existing: RevisionConfig = env.storage().persistent()
            .get(&DataKey::RevisionConfig(commission_id.clone()))
            .unwrap_or(RevisionConfig { max_revisions: 0, used_revisions: 0 });

        let config = RevisionConfig { max_revisions, used_revisions: existing.used_revisions };
        env.storage().persistent().set(&DataKey::RevisionConfig(commission_id.clone()), &config);

        env.events().publish((symbol_short!("rev_lim"),), (commission_id, max_revisions));
        Ok(())
    }

    /// Submit a revision request on an Active agreement. Closes #600.
    pub fn request_revision(
        env: Env,
        revision_id: Bytes,
        commission_id: Bytes,
        proposer_addr: Address,
        description: String,
        cost_adjustment_usdc: i128,
        deadline_ledger: u32,
    ) -> Result<(), AgreementError> {
        proposer_addr.require_auth();

        let record = load_agreement(&env, &commission_id)?;
        if record.status != AgreementStatus::Active {
            return Err(AgreementError::InvalidStatus);
        }
        if proposer_addr != record.client && proposer_addr != record.artist {
            return Err(AgreementError::Unauthorized);
        }
        if deadline_ledger <= env.ledger().sequence() {
            return Err(AgreementError::RevisionDeadlinePast);
        }
        if env.storage().persistent().has(&DataKey::Revision(commission_id.clone(), revision_id.clone())) {
            return Err(AgreementError::RevisionAlreadyExists);
        }

        let mut config: RevisionConfig = env.storage().persistent()
            .get(&DataKey::RevisionConfig(commission_id.clone()))
            .unwrap_or(RevisionConfig { max_revisions: 0, used_revisions: 0 });

        if config.max_revisions > 0 && config.used_revisions >= config.max_revisions {
            return Err(AgreementError::RevisionLimitReached);
        }

        let proposer = if proposer_addr == record.artist {
            RevisionProposer::Artist
        } else {
            RevisionProposer::Client
        };

        let revision = RevisionRecord {
            revision_id: revision_id.clone(),
            commission_id: commission_id.clone(),
            proposer,
            description,
            cost_adjustment_usdc,
            deadline_ledger,
            status: RevisionStatus::Pending,
            created_ledger: env.ledger().sequence(),
        };

        env.storage().persistent().set(
            &DataKey::Revision(commission_id.clone(), revision_id.clone()),
            &revision,
        );

        let mut revisions: Vec<RevisionRecord> = env.storage().persistent()
            .get(&DataKey::RevisionsForAgreement(commission_id.clone()))
            .unwrap_or(Vec::new(&env));
        revisions.push_back(revision);
        env.storage().persistent().set(
            &DataKey::RevisionsForAgreement(commission_id.clone()),
            &revisions,
        );

        config.used_revisions = config.used_revisions.saturating_add(1);
        env.storage().persistent().set(&DataKey::RevisionConfig(commission_id.clone()), &config);

        env.events().publish(
            (symbol_short!("rev_req"),),
            (revision_id, commission_id, proposer_addr, cost_adjustment_usdc),
        );
        Ok(())
    }

    /// Accept a pending revision. Counterparty only. Closes #600.
    pub fn accept_revision(
        env: Env,
        commission_id: Bytes,
        revision_id: Bytes,
        responder: Address,
    ) -> Result<(), AgreementError> {
        responder.require_auth();

        let mut record = load_agreement(&env, &commission_id)?;
        if record.status != AgreementStatus::Active {
            return Err(AgreementError::InvalidStatus);
        }
        if responder != record.client && responder != record.artist {
            return Err(AgreementError::Unauthorized);
        }

        let mut revision: RevisionRecord = env.storage().persistent()
            .get(&DataKey::Revision(commission_id.clone(), revision_id.clone()))
            .ok_or(AgreementError::NotFound)?;

        if revision.status != RevisionStatus::Pending {
            return Err(AgreementError::RevisionNotPending);
        }

        let proposer_is_artist = matches!(revision.proposer, RevisionProposer::Artist);
        let responder_is_artist = responder == record.artist;
        if proposer_is_artist == responder_is_artist {
            return Err(AgreementError::Unauthorized);
        }

        if revision.cost_adjustment_usdc != 0 {
            record.budget_usdc = record.budget_usdc
                .checked_add(revision.cost_adjustment_usdc)
                .ok_or(AgreementError::ArithmeticOverflow)?;
            if record.budget_usdc <= 0 {
                return Err(AgreementError::InvalidAmount);
            }
            env.storage().persistent().set(&DataKey::Agreement(commission_id.clone()), &record);
        }

        revision.status = RevisionStatus::Accepted;
        env.storage().persistent().set(
            &DataKey::Revision(commission_id.clone(), revision_id.clone()),
            &revision,
        );

        env.events().publish(
            (symbol_short!("rev_acc"),),
            (revision_id, commission_id, revision.cost_adjustment_usdc),
        );
        Ok(())
    }

    /// Reject a pending revision. Counterparty only. Closes #600.
    pub fn reject_revision(
        env: Env,
        commission_id: Bytes,
        revision_id: Bytes,
        responder: Address,
    ) -> Result<(), AgreementError> {
        responder.require_auth();

        let record = load_agreement(&env, &commission_id)?;
        if record.status != AgreementStatus::Active {
            return Err(AgreementError::InvalidStatus);
        }
        if responder != record.client && responder != record.artist {
            return Err(AgreementError::Unauthorized);
        }

        let mut revision: RevisionRecord = env.storage().persistent()
            .get(&DataKey::Revision(commission_id.clone(), revision_id.clone()))
            .ok_or(AgreementError::NotFound)?;

        if revision.status != RevisionStatus::Pending {
            return Err(AgreementError::RevisionNotPending);
        }

        let proposer_is_artist = matches!(revision.proposer, RevisionProposer::Artist);
        let responder_is_artist = responder == record.artist;
        if proposer_is_artist == responder_is_artist {
            return Err(AgreementError::Unauthorized);
        }

        revision.status = RevisionStatus::Rejected;
        env.storage().persistent().set(
            &DataKey::Revision(commission_id.clone(), revision_id.clone()),
            &revision,
        );

        env.events().publish((symbol_short!("rev_rej"),), (revision_id, commission_id));
        Ok(())
    }

    /// Expire a pending revision whose deadline has passed. Closes #600.
    pub fn expire_revision(
        env: Env,
        commission_id: Bytes,
        revision_id: Bytes,
    ) -> Result<(), AgreementError> {
        let mut revision: RevisionRecord = env.storage().persistent()
            .get(&DataKey::Revision(commission_id.clone(), revision_id.clone()))
            .ok_or(AgreementError::NotFound)?;

        if revision.status != RevisionStatus::Pending {
            return Err(AgreementError::RevisionNotPending);
        }
        if env.ledger().sequence() < revision.deadline_ledger {
            return Err(AgreementError::RevisionDeadlinePast);
        }

        revision.status = RevisionStatus::Expired;
        env.storage().persistent().set(
            &DataKey::Revision(commission_id.clone(), revision_id.clone()),
            &revision,
        );

        env.events().publish((symbol_short!("rev_exp"),), (revision_id, commission_id));
        Ok(())
    }

    /// Return all revision records for an agreement. Closes #600.
    pub fn get_revisions(env: Env, commission_id: Bytes) -> Result<Vec<RevisionRecord>, AgreementError> {
        if !env.storage().persistent().has(&DataKey::Agreement(commission_id.clone())) {
            return Err(AgreementError::NotFound);
        }
        Ok(env.storage().persistent()
            .get(&DataKey::RevisionsForAgreement(commission_id))
            .unwrap_or(Vec::new(&env)))
    }

    /// Return the revision configuration for an agreement. Closes #600.
    pub fn get_revision_config(env: Env, commission_id: Bytes) -> Result<RevisionConfig, AgreementError> {
        if !env.storage().persistent().has(&DataKey::Agreement(commission_id.clone())) {
            return Err(AgreementError::NotFound);
        }
        Ok(env.storage().persistent()
            .get(&DataKey::RevisionConfig(commission_id))
            .unwrap_or(RevisionConfig { max_revisions: 0, used_revisions: 0 }))
    }

    // ── Team Collaboration — closes #603 ───────────────────────────────────

    const MAX_TEAM_SIZE: u32 = 20;

    /// Add a team member to an active agreement. Lead only. Closes #603.
    pub fn add_team_member(
        env: Env,
        commission_id: Bytes,
        lead: Address,
        new_member: Address,
        role: TeamRole,
        attribution: String,
    ) -> Result<(), AgreementError> {
        lead.require_auth();

        let record = load_agreement(&env, &commission_id)?;
        if record.status != AgreementStatus::Active {
            return Err(AgreementError::InvalidStatus);
        }
        if lead != record.artist {
            return Err(AgreementError::TeamLeadRequired);
        }
        if matches!(role, TeamRole::Lead) {
            return Err(AgreementError::TeamLeadRequired);
        }

        let mut members: Vec<TeamMember> = env.storage().persistent()
            .get(&DataKey::TeamMembers(commission_id.clone()))
            .unwrap_or(Vec::new(&env));

        if members.len() as u32 >= Self::MAX_TEAM_SIZE {
            return Err(AgreementError::MaxTeamSizeExceeded);
        }
        for m in members.iter() {
            if m.member == new_member {
                return Err(AgreementError::TeamMemberAlreadyExists);
            }
        }

        members.push_back(TeamMember {
            member: new_member.clone(),
            role,
            attribution,
            added_ledger: env.ledger().sequence(),
        });
        env.storage().persistent().set(&DataKey::TeamMembers(commission_id.clone()), &members);

        env.events().publish(
            (symbol_short!("tm_add"),),
            (commission_id, new_member, role as u32),
        );
        Ok(())
    }

    /// Remove a team member. Lead only. Closes #603.
    pub fn remove_team_member(
        env: Env,
        commission_id: Bytes,
        lead: Address,
        member_to_remove: Address,
    ) -> Result<(), AgreementError> {
        lead.require_auth();

        let record = load_agreement(&env, &commission_id)?;
        if record.status != AgreementStatus::Active {
            return Err(AgreementError::InvalidStatus);
        }
        if lead != record.artist {
            return Err(AgreementError::TeamLeadRequired);
        }
        if member_to_remove == record.artist {
            return Err(AgreementError::Unauthorized);
        }

        let members: Vec<TeamMember> = env.storage().persistent()
            .get(&DataKey::TeamMembers(commission_id.clone()))
            .unwrap_or(Vec::new(&env));
        let mut updated: Vec<TeamMember> = Vec::new(&env);
        let mut found = false;
        for m in members.iter() {
            if m.member == member_to_remove {
                found = true;
            } else {
                updated.push_back(m);
            }
        }
        if !found {
            return Err(AgreementError::TeamMemberNotFound);
        }

        env.storage().persistent().set(&DataKey::TeamMembers(commission_id.clone()), &updated);
        env.events().publish((symbol_short!("tm_rm"),), (commission_id, member_to_remove));
        Ok(())
    }

    /// Update a team member's role. Lead only. Closes #603.
    pub fn update_team_member_role(
        env: Env,
        commission_id: Bytes,
        lead: Address,
        member_addr: Address,
        new_role: TeamRole,
    ) -> Result<(), AgreementError> {
        lead.require_auth();

        let record = load_agreement(&env, &commission_id)?;
        if record.status != AgreementStatus::Active {
            return Err(AgreementError::InvalidStatus);
        }
        if lead != record.artist {
            return Err(AgreementError::TeamLeadRequired);
        }
        if matches!(new_role, TeamRole::Lead) {
            return Err(AgreementError::TeamLeadRequired);
        }

        let members: Vec<TeamMember> = env.storage().persistent()
            .get(&DataKey::TeamMembers(commission_id.clone()))
            .unwrap_or(Vec::new(&env));
        let mut updated: Vec<TeamMember> = Vec::new(&env);
        let mut found = false;
        for m in members.iter() {
            if m.member == member_addr {
                found = true;
                updated.push_back(TeamMember {
                    member: m.member.clone(),
                    role: new_role,
                    attribution: m.attribution.clone(),
                    added_ledger: m.added_ledger,
                });
            } else {
                updated.push_back(m);
            }
        }
        if !found {
            return Err(AgreementError::TeamMemberNotFound);
        }

        env.storage().persistent().set(&DataKey::TeamMembers(commission_id.clone()), &updated);
        env.events().publish((symbol_short!("tm_role"),), (commission_id, member_addr, new_role as u32));
        Ok(())
    }

    /// Configure how the artist's payout is split among team members.
    ///
    /// `entries` must sum to exactly 10_000 bps. Closes #603.
    pub fn set_payment_split(
        env: Env,
        commission_id: Bytes,
        lead: Address,
        entries: Vec<PaymentSplitEntry>,
    ) -> Result<(), AgreementError> {
        lead.require_auth();

        let record = load_agreement(&env, &commission_id)?;
        if record.status != AgreementStatus::Active {
            return Err(AgreementError::InvalidStatus);
        }
        if lead != record.artist {
            return Err(AgreementError::TeamLeadRequired);
        }
        if entries.is_empty() {
            return Err(AgreementError::InvalidPaymentSplit);
        }

        let total_bps: u32 = entries.iter().map(|e| e.share_bps).sum();
        if total_bps != 10_000 {
            return Err(AgreementError::InvalidPaymentSplit);
        }

        let members: Vec<TeamMember> = env.storage().persistent()
            .get(&DataKey::TeamMembers(commission_id.clone()))
            .unwrap_or(Vec::new(&env));

        for entry in entries.iter() {
            if entry.member == record.artist {
                continue;
            }
            let found = members.iter().any(|m| m.member == entry.member);
            if !found {
                return Err(AgreementError::TeamMemberNotFound);
            }
        }

        env.storage().persistent().set(&DataKey::PaymentSplitConfig(commission_id.clone()), &entries);
        env.events().publish((symbol_short!("tm_splt"),), (commission_id, total_bps));
        Ok(())
    }

    /// Return all team members for an agreement. Closes #603.
    pub fn get_team_members(env: Env, commission_id: Bytes) -> Result<Vec<TeamMember>, AgreementError> {
        if !env.storage().persistent().has(&DataKey::Agreement(commission_id.clone())) {
            return Err(AgreementError::NotFound);
        }
        Ok(env.storage().persistent()
            .get(&DataKey::TeamMembers(commission_id))
            .unwrap_or(Vec::new(&env)))
    }

    /// Return the payment split configuration. Closes #603.
    pub fn get_payment_split(env: Env, commission_id: Bytes) -> Result<Vec<PaymentSplitEntry>, AgreementError> {
        if !env.storage().persistent().has(&DataKey::Agreement(commission_id.clone())) {
            return Err(AgreementError::NotFound);
        }
        Ok(env.storage().persistent()
            .get(&DataKey::PaymentSplitConfig(commission_id))
            .unwrap_or(Vec::new(&env)))
    }
}

impl CommissionAgreementContract {
    fn in_grace(env: &Env, record: &AgreementRecord, policy: &CancellationPolicy) -> bool {
        policy.grace_ledgers > 0
            && env.ledger().sequence() <= record.created_ledger + policy.grace_ledgers
    }
}

#[cfg(all(test, feature = "legacy_tests"))]
mod integration_tests;
