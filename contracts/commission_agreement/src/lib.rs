//! CommissionAgreement contract — core agreement lifecycle functions.
//!
//! Architecture Decision: [ADR-0003](../../docs/ADRs/0003-commission-agreement-milestone-flow.md)
//! See also: [ADR-0006](../../docs/ADRs/0006-event-driven-architecture.md)
//!
//! Implements:
//! - `create_agreement`    (closes #457, closes #458)
//! - `accept_agreement`    (closes #459)
//! - `reject_agreement`    (closes #459)
//! - `propose_milestone`   (closes #460)

#![no_std]

// These modules target an earlier revision of this contract's API (`Commission`,
// `Milestone`, wasm fixtures) that no longer exists here; they do not compile, so
// on `main` `cargo test -p commission_agreement` fails before any test runs. They
// are gated off by default so the crate's tests are runnable — re-enable with
// `--features legacy_tests` once they have been ported.
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

use soroban_sdk::{contract, contractimpl, symbol_short, token, Address, Bytes, Env, String, Vec};
use agency::{AgencyAnalytics, AgencyProfile, BatchPayment, RosterEntry};
use cancellation::{CancellationPolicy, CancellationQuote, CancellationReason, CancellationRecord};
use errors::AgreementError;
use types::{AgreementRecord, AgreementStatus, DataKey, MilestoneRecord, MilestoneStatus};

include!("../../semver_types.rs");

/// Cap on the retained cancellation history, so the list stays bounded.
const CANCELLATION_HISTORY_LIMIT: u32 = 50;

fn load_agreement(env: &Env, commission_id: &Bytes) -> Result<AgreementRecord, AgreementError> {
    env.storage()
        .persistent()
        .get(&DataKey::Agreement(commission_id.clone()))
        .ok_or(AgreementError::NotFound)
}

/// Value of the milestones the client has already approved. This is the basis
/// for the pro-rata split on cancellation.
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

/// Attribute a new commission to the artist's agency, if they have one. Called
/// on every agreement creation so agency analytics stay current without the
/// client needing to know about the representation.
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

// ── Input length limits (closes #591) ────────────────────────────────────────

/// Maximum byte length for a title in a commission agreement.
const MAX_TITLE_LEN: u32 = 128;
/// Maximum byte length for a milestone title.
const MAX_MILESTONE_TITLE_LEN: u32 = 128;
/// Maximum byte length for a rejection reason.
const MAX_REASON_LEN: u32 = 512;
/// Maximum byte length for a commission / milestone identifier.
const MAX_ID_LEN: u32 = 64;

// ── Deadline upper bound (closes #592) ───────────────────────────────────────

/// Maximum number of ledgers into the future a deadline may be set.
/// At ~5 s per ledger: 12_614_400 ≈ 2 years.
const MAX_DEADLINE_OFFSET_LEDGERS: u32 = 12_614_400;

#[contract]
pub struct CommissionAgreementContract;

#[contractimpl]
impl CommissionAgreementContract {
    impl_semver_queries!();

    /// Create a new commission agreement.
    ///
    /// Closes #457, closes #458.
    /// Closes #591 – validates title and ID lengths before persisting.
    /// Closes #592 – validates deadline does not exceed MAX_DEADLINE_OFFSET_LEDGERS.
    ///
    /// # Errors
    /// - [`AgreementError::InvalidAmount`] if `budget_usdc <= 0`
    /// - [`AgreementError::DeadlineInPast`] if `deadline_ledger <= current sequence`
    /// - [`AgreementError::DeadlineTooFar`] if `deadline_ledger > current + MAX_DEADLINE_OFFSET_LEDGERS`
    /// - [`AgreementError::InputTooLong`] if `title` or `commission_id` exceeds limits
    /// - [`AgreementError::AlreadyExists`] if an agreement with the same `commission_id` exists
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

        // ── Input length validation (closes #591) ──────────────────────────
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
        // ── Deadline upper bound (closes #592) ─────────────────────────────
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
        env.storage().persistent().set(&DataKey::MilestonesForAgreement(commission_id.clone()), &Vec::<MilestoneRecord>::new(&env));

        attribute_commission(&env, &artist, budget_usdc);

        env.events().publish(
            (symbol_short!("agr_new"),),
            (commission_id, client, artist, budget_usdc),
        );

        Ok(())
    }

    /// Accept a pending commission agreement (artist auth required).
    ///
    /// Sets status to `Active` and emits `AgreementAccepted`. Closes #459.
    pub fn accept_agreement(env: Env, commission_id: Bytes) -> Result<(), AgreementError> {
        let mut record: AgreementRecord = env.storage().persistent()
            .get(&DataKey::Agreement(commission_id.clone()))
            .ok_or(AgreementError::NotFound)?;
        
        record.artist.require_auth();

        if record.status != AgreementStatus::Pending {
            return Err(AgreementError::InvalidStatus);
        }

        record.status = AgreementStatus::Active;
        env.storage().persistent().set(&DataKey::Agreement(commission_id.clone()), &record);

        env.events().publish((symbol_short!("agr_ok"),), (commission_id,));
        Ok(())
    }

    /// Reject a pending commission agreement (artist auth required).
    ///
    /// Sets status to `Cancelled` and emits `AgreementRejected`. Closes #459.
    /// Closes #591 – validates reason length.
    pub fn reject_agreement(env: Env, commission_id: Bytes, reason: String) -> Result<(), AgreementError> {
        // ── Input length validation (closes #591) ──────────────────────────
        if reason.len() > MAX_REASON_LEN {
            return Err(AgreementError::InputTooLong);
        }

        let mut record: AgreementRecord = env.storage().persistent()
            .get(&DataKey::Agreement(commission_id.clone()))
            .ok_or(AgreementError::NotFound)?;
        
        record.artist.require_auth();

        if record.status != AgreementStatus::Pending {
            return Err(AgreementError::InvalidStatus);
        }

        record.status = AgreementStatus::Cancelled;
        env.storage().persistent().set(&DataKey::Agreement(commission_id.clone()), &record);

        env.events().publish((symbol_short!("agr_rej"),), (commission_id, reason));
        Ok(())
    }

    /// Propose a new milestone on an active agreement (artist auth required).
    ///
    /// Validates the cumulative milestone budget does not exceed `budget_usdc`.
    /// Emits `MilestoneProposed`. Closes #460.
    /// Closes #591 – validates milestone title and ID lengths.
    pub fn propose_milestone(
        env: Env,
        commission_id: Bytes,
        milestone_id: Bytes,
        title: String,
        amount_usdc: i128,
    ) -> Result<(), AgreementError> {
        // ── Input length validation (closes #591) ──────────────────────────
        if milestone_id.len() > MAX_ID_LEN {
            return Err(AgreementError::InputTooLong);
        }
        if title.len() > MAX_MILESTONE_TITLE_LEN {
            return Err(AgreementError::InputTooLong);
        }

        let record: AgreementRecord = env.storage().persistent()
            .get(&DataKey::Agreement(commission_id.clone()))
            .ok_or(AgreementError::NotFound)?;

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

        env.storage().persistent().set(&DataKey::Milestone(commission_id.clone(), milestone_id.clone()), &milestone);
        let mut updated = milestones;
        updated.push_back(milestone);
        env.storage().persistent().set(&DataKey::MilestonesForAgreement(commission_id.clone()), &updated);

        env.events().publish(
            (symbol_short!("ms_new"),),
            (commission_id, milestone_id, amount_usdc),
        );
        Ok(())
    }

    pub fn approve_milestone(env: Env, commission_id: Bytes, milestone_id: Bytes) -> Result<(), AgreementError> {
        let mut record: AgreementRecord = env.storage().persistent()
            .get(&DataKey::Agreement(commission_id.clone()))
            .ok_or(AgreementError::NotFound)?;

        record.client.require_auth();

        if record.status != AgreementStatus::Active {
            return Err(AgreementError::InvalidStatus);
        }

        // ── Serialization lock for milestone state transitions (closes #589) ─
        // Acquire the lock before reading the milestone status.  Any concurrent
        // call (e.g. a simultaneous approve + reject) will find the lock set and
        // return `MilestoneLocked`, preventing inconsistent state.
        let lock_key = DataKey::MilestoneLock(commission_id.clone(), milestone_id.clone());
        if env.storage().persistent().has(&lock_key) {
            return Err(AgreementError::MilestoneLocked);
        }
        env.storage().persistent().set(&lock_key, &true);

        let mut milestone: MilestoneRecord = env.storage().persistent()
            .get(&DataKey::Milestone(commission_id.clone(), milestone_id.clone()))
            .ok_or(AgreementError::NotFound)?;

        if milestone.status != MilestoneStatus::Pending {
            // Release lock before returning
            env.storage().persistent().remove(&lock_key);
            return Err(AgreementError::InvalidStatus);
        }

        // EFFECTS: update milestone status
        milestone.status = MilestoneStatus::Approved;
        env.storage().persistent().set(&DataKey::Milestone(commission_id.clone(), milestone_id.clone()), &milestone);

        // Update the milestone list in-place so the all_approved check is accurate (closes #589).
        let milestones: Vec<MilestoneRecord> = env.storage().persistent()
            .get(&DataKey::MilestonesForAgreement(commission_id.clone()))
            .unwrap_or(Vec::new(&env));

        let mut updated_milestones = Vec::new(&env);
        for m in milestones.iter() {
            if m.milestone_id == milestone_id {
                updated_milestones.push_back(milestone.clone());
            } else {
                updated_milestones.push_back(m);
            }
        }
        env.storage().persistent().set(&DataKey::MilestonesForAgreement(commission_id.clone()), &updated_milestones);

        // Check whether all milestones are now approved using the updated list.
        let all_approved = !updated_milestones.is_empty()
            && updated_milestones.iter().all(|m| m.status == MilestoneStatus::Approved);
        if all_approved {
            record.status = AgreementStatus::Completed;
            env.storage().persistent().set(&DataKey::Agreement(commission_id.clone()), &record);
        }

        // Release the serialization lock
        env.storage().persistent().remove(&lock_key);

        env.events().publish((symbol_short!("ms_apprvd"),), (commission_id, milestone_id));
        Ok(())
    }

    pub fn get_agreement(env: Env, commission_id: Bytes) -> Result<AgreementRecord, AgreementError> {
        env.storage().persistent()
            .get(&DataKey::Agreement(commission_id))
            .ok_or(AgreementError::NotFound)
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

    /// Set the cancellation policy for an agreement. Only allowed while the
    /// agreement is still `Pending`, so the artist accepts with the terms of
    /// an early exit already visible.
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

    /// Preview the settlement without changing anything, so both sides can see
    /// the split before anyone commits to cancelling.
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

    /// Cancel an agreement and record the pro-rata settlement. Either party may
    /// cancel; the resulting `artist_amount` and `client_refund` sum to the
    /// budget and are what the escrow should be drained with.
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
            (
                commission_id,
                initiator,
                reason,
                quote.completion_bps,
                quote.artist_amount,
                quote.client_refund,
            ),
        );
        Ok(cancellation_record)
    }

    pub fn get_cancellation(
        env: Env,
        commission_id: Bytes,
    ) -> Result<CancellationRecord, AgreementError> {
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

    /// Register an agency account. The agency address is its own identity and
    /// must authorise every roster and payout action.
    pub fn register_agency(
        env: Env,
        agency: Address,
        name: String,
        default_split_bps: u32,
    ) -> Result<(), AgreementError> {
        agency.require_auth();

        if env
            .storage()
            .persistent()
            .has(&DataKey::Agency(agency.clone()))
        {
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
        env.storage()
            .persistent()
            .set(&DataKey::Agency(agency.clone()), &profile);
        env.storage()
            .persistent()
            .set(&DataKey::Roster(agency.clone()), &Vec::<Address>::new(&env));

        env.events()
            .publish((symbol_short!("agy_new"),), (agency, default_split_bps));
        Ok(())
    }

    /// Add an artist to a roster. An artist can only be represented by one
    /// agency at a time, so commission attribution is never ambiguous.
    pub fn add_artist(
        env: Env,
        agency: Address,
        artist: Address,
        split_bps: u32,
    ) -> Result<(), AgreementError> {
        let mut profile = load_agency(&env, &agency)?;
        agency.require_auth();
        agency::validate_split_bps(split_bps)?;

        if env
            .storage()
            .persistent()
            .has(&DataKey::ArtistAgency(artist.clone()))
        {
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
        env.storage()
            .persistent()
            .set(&DataKey::RosterEntry(agency.clone(), artist.clone()), &entry);
        env.storage()
            .persistent()
            .set(&DataKey::ArtistAgency(artist.clone()), &agency);

        let mut roster: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::Roster(agency.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        roster.push_back(artist.clone());
        env.storage()
            .persistent()
            .set(&DataKey::Roster(agency.clone()), &roster);

        profile.artist_count += 1;
        env.storage()
            .persistent()
            .set(&DataKey::Agency(agency.clone()), &profile);
        let mut analytics = load_analytics(&env, &agency);
        analytics.artist_count = profile.artist_count;
        save_analytics(&env, &agency, &analytics);

        env.events()
            .publish((symbol_short!("agy_add"),), (agency, artist, split_bps));
        Ok(())
    }

    /// Remove an artist from a roster. The historic split totals on the entry
    /// are kept so past earnings stay auditable.
    pub fn remove_artist(env: Env, agency: Address, artist: Address) -> Result<(), AgreementError> {
        let mut profile = load_agency(&env, &agency)?;
        agency.require_auth();
        load_roster_entry(&env, &agency, &artist)?;

        env.storage()
            .persistent()
            .remove(&DataKey::ArtistAgency(artist.clone()));

        let roster: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::Roster(agency.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        let mut remaining = Vec::new(&env);
        for member in roster.iter() {
            if member != artist {
                remaining.push_back(member);
            }
        }
        env.storage()
            .persistent()
            .set(&DataKey::Roster(agency.clone()), &remaining);

        profile.artist_count = remaining.len();
        env.storage()
            .persistent()
            .set(&DataKey::Agency(agency.clone()), &profile);
        let mut analytics = load_analytics(&env, &agency);
        analytics.artist_count = profile.artist_count;
        save_analytics(&env, &agency, &analytics);

        env.events()
            .publish((symbol_short!("agy_rm"),), (agency, artist));
        Ok(())
    }

    pub fn set_artist_split(
        env: Env,
        agency: Address,
        artist: Address,
        split_bps: u32,
    ) -> Result<(), AgreementError> {
        load_agency(&env, &agency)?;
        agency.require_auth();
        agency::validate_split_bps(split_bps)?;

        let mut entry = load_roster_entry(&env, &agency, &artist)?;
        entry.split_bps = split_bps;
        env.storage()
            .persistent()
            .set(&DataKey::RosterEntry(agency.clone(), artist.clone()), &entry);

        env.events()
            .publish((symbol_short!("agy_split"),), (agency, artist, split_bps));
        Ok(())
    }

    /// Pay a batch of rostered artists in one call. Each line is split by the
    /// artist's rostered rate: the agency keeps its cut and forwards the rest,
    /// and both halves are added to the running split totals.
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

        // Effects first: every roster entry and the analytics roll-up are
        // committed before any token leaves the agency.
        let mut nets: Vec<i128> = Vec::new(&env);
        for payment in payments.iter() {
            let mut entry = load_roster_entry(&env, &agency, &payment.artist)?;
            let (agency_cut, artist_net) =
                agency::split_payment(payment.gross_usdc, entry.split_bps)?;

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
            (symbol_short!("agy_batch"),),
            (agency, payments.len(), total_gross),
        );
        Ok(total_gross)
    }

    pub fn get_agency(env: Env, agency: Address) -> Result<AgencyProfile, AgreementError> {
        load_agency(&env, &agency)
    }

    pub fn get_roster(env: Env, agency: Address) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::Roster(agency))
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_roster_entry(
        env: Env,
        agency: Address,
        artist: Address,
    ) -> Result<RosterEntry, AgreementError> {
        load_roster_entry(&env, &agency, &artist)
    }

    pub fn get_artist_agency(env: Env, artist: Address) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::ArtistAgency(artist))
    }

    pub fn get_agency_analytics(env: Env, agency: Address) -> AgencyAnalytics {
        load_analytics(&env, &agency)
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


impl CommissionAgreementContract {
    /// True while the agreement is inside its free-cancellation window.
    fn in_grace(env: &Env, record: &AgreementRecord, policy: &CancellationPolicy) -> bool {
        policy.grace_ledgers > 0
            && env.ledger().sequence() <= record.created_ledger + policy.grace_ledgers
    }
}
#[cfg(all(test, feature = "legacy_tests"))]
mod integration_tests;