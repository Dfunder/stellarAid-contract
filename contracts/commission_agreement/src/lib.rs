//! CommissionAgreement contract — core agreement lifecycle functions.
//!
//! Implements:
//! - `create_agreement`    (closes #457, closes #458)
//! - `accept_agreement`    (closes #459)
//! - `reject_agreement`    (closes #459)
//! - `propose_milestone`   (closes #460)

#![no_std]

#[cfg(test)]
mod test;

#[cfg(test)]
mod milestone_flow;

#[cfg(test)]
mod multiple_escrows;

#[cfg(test)]
mod dispute_resolution;

mod errors;
mod types;

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Bytes, Env, String, Vec};
use types::{AgreementRecord, AgreementStatus, DataKey, MilestoneRecord, MilestoneStatus,
            TeamMember, TeamRole, InvitationStatus};
use errors::AgreementError;

#[contract]
pub struct CommissionAgreementContract;

#[contractimpl]
impl CommissionAgreementContract {
    /// Create a new commission agreement.
    ///
    /// Closes #457, closes #458.
    ///
    /// # Errors
    /// - [`AgreementError::InvalidAmount`] if `budget_usdc <= 0`
    /// - [`AgreementError::DeadlineInPast`] if `deadline_ledger <= current sequence`
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

        if budget_usdc <= 0 {
            return Err(AgreementError::InvalidAmount);
        }
        if deadline_ledger <= env.ledger().sequence() {
            return Err(AgreementError::DeadlineInPast);
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

        env.events().publish(
            (symbol_short!("agr_crtd"),),
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

        env.events().publish((symbol_short!("agr_acpt"),), (commission_id,));
        Ok(())
    }

    /// Reject a pending commission agreement (artist auth required).
    ///
    /// Sets status to `Cancelled` and emits `AgreementRejected`. Closes #459.
    pub fn reject_agreement(env: Env, commission_id: Bytes, reason: String) -> Result<(), AgreementError> {
        let mut record: AgreementRecord = env.storage().persistent()
            .get(&DataKey::Agreement(commission_id.clone()))
            .ok_or(AgreementError::NotFound)?;
        
        record.artist.require_auth();

        if record.status != AgreementStatus::Pending {
            return Err(AgreementError::InvalidStatus);
        }

        record.status = AgreementStatus::Cancelled;
        env.storage().persistent().set(&DataKey::Agreement(commission_id.clone()), &record);

        env.events().publish((symbol_short!("agr_rjct"),), (commission_id, reason));
        Ok(())
    }

    /// Propose a new milestone on an active agreement (artist auth required).
    ///
    /// Validates the cumulative milestone budget does not exceed `budget_usdc`.
    /// Emits `MilestoneProposed`. Closes #460.
    pub fn propose_milestone(
        env: Env,
        commission_id: Bytes,
        milestone_id: Bytes,
        title: String,
        amount_usdc: i128,
    ) -> Result<(), AgreementError> {
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
            (symbol_short!("ms_prop"),),
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

        let mut milestone: MilestoneRecord = env.storage().persistent()
            .get(&DataKey::Milestone(commission_id.clone(), milestone_id.clone()))
            .ok_or(AgreementError::NotFound)?;

        if milestone.status != MilestoneStatus::Pending {
            return Err(AgreementError::InvalidStatus);
        }

        milestone.status = MilestoneStatus::Approved;
        env.storage().persistent().set(&DataKey::Milestone(commission_id.clone(), milestone_id.clone()), &milestone);

        let milestones: Vec<MilestoneRecord> = env.storage().persistent()
            .get(&DataKey::MilestonesForAgreement(commission_id.clone()))
            .unwrap_or(Vec::new(&env));
        let all_approved = milestones.iter().all(|m| m.status == MilestoneStatus::Approved || m.milestone_id == milestone_id);
        if all_approved && !milestones.is_empty() {
            record.status = AgreementStatus::Completed;
            env.storage().persistent().set(&DataKey::Agreement(commission_id.clone()), &record);
        }

        env.events().publish((symbol_short!("ms_appr"),), (commission_id, milestone_id));
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

    // ── Team Collaboration (closes #603) ────────────────────────────────────

    /// Invite a team member to a commission agreement.
    ///
    /// Only the lead artist (the `artist` field of the agreement) may invite
    /// new members. The agreement must be Active.
    ///
    /// `payment_share_bps` is this member's share of the artist payout (0–10000
    /// basis points). The sum of all member shares must not exceed 10 000 bps.
    ///
    /// Closes #603 – team member invitation flow and payment split configuration.
    pub fn invite_team_member(
        env: Env,
        commission_id: Bytes,
        member: Address,
        role: TeamRole,
        payment_share_bps: u32,
        contribution_note: String,
    ) -> Result<(), AgreementError> {
        let record: AgreementRecord = env.storage().persistent()
            .get(&DataKey::Agreement(commission_id.clone()))
            .ok_or(AgreementError::NotFound)?;

        record.artist.require_auth();

        if record.status != AgreementStatus::Active {
            return Err(AgreementError::InvalidStatus);
        }
        if payment_share_bps > 10_000 {
            return Err(AgreementError::InvalidAmount);
        }

        let mut members: Vec<TeamMember> = env.storage().persistent()
            .get(&DataKey::TeamMembers(commission_id.clone()))
            .unwrap_or(Vec::new(&env));

        // Enforce max team size
        if members.len() >= 10 {
            return Err(AgreementError::TeamSizeLimit);
        }

        // Prevent duplicate
        for m in members.iter() {
            if m.member == member {
                return Err(AgreementError::MemberAlreadyExists);
            }
        }

        // Validate cumulative share
        let total_share: u32 = members.iter().map(|m| m.payment_share_bps).sum();
        let new_total = total_share
            .checked_add(payment_share_bps)
            .ok_or(AgreementError::ArithmeticOverflow)?;
        if new_total > 10_000 {
            return Err(AgreementError::PaymentShareExceeded);
        }

        members.push_back(TeamMember {
            member: member.clone(),
            role,
            invitation_status: InvitationStatus::Pending,
            payment_share_bps,
            contribution_note,
            added_ledger: env.ledger().sequence(),
        });
        env.storage().persistent().set(&DataKey::TeamMembers(commission_id.clone()), &members);

        env.events().publish(
            (symbol_short!("tm_invite"),),
            (commission_id, member, payment_share_bps),
        );
        Ok(())
    }

    /// Accept a pending team invitation.
    ///
    /// Only the invited member themselves may accept their own invitation.
    ///
    /// Closes #603 – team member invitation flow.
    pub fn accept_team_invitation(
        env: Env,
        commission_id: Bytes,
        member: Address,
    ) -> Result<(), AgreementError> {
        member.require_auth();

        let members: Vec<TeamMember> = env.storage().persistent()
            .get(&DataKey::TeamMembers(commission_id.clone()))
            .ok_or(AgreementError::NotFound)?;

        let mut found = false;
        let mut updated = Vec::new(&env);
        for mut m in members.iter() {
            if m.member == member {
                if m.invitation_status != InvitationStatus::Pending {
                    return Err(AgreementError::InvalidInvitationStatus);
                }
                m.invitation_status = InvitationStatus::Accepted;
                found = true;
            }
            updated.push_back(m);
        }
        if !found {
            return Err(AgreementError::NotFound);
        }
        env.storage().persistent().set(&DataKey::TeamMembers(commission_id.clone()), &updated);

        env.events().publish(
            (symbol_short!("tm_accept"),),
            (commission_id, member),
        );
        Ok(())
    }

    /// Decline a pending team invitation.
    ///
    /// Only the invited member may decline their own invitation.
    ///
    /// Closes #603 – team member invitation flow.
    pub fn decline_team_invitation(
        env: Env,
        commission_id: Bytes,
        member: Address,
    ) -> Result<(), AgreementError> {
        member.require_auth();

        let members: Vec<TeamMember> = env.storage().persistent()
            .get(&DataKey::TeamMembers(commission_id.clone()))
            .ok_or(AgreementError::NotFound)?;

        let mut found = false;
        let mut updated = Vec::new(&env);
        for mut m in members.iter() {
            if m.member == member {
                if m.invitation_status != InvitationStatus::Pending {
                    return Err(AgreementError::InvalidInvitationStatus);
                }
                m.invitation_status = InvitationStatus::Declined;
                found = true;
            }
            updated.push_back(m);
        }
        if !found {
            return Err(AgreementError::NotFound);
        }
        env.storage().persistent().set(&DataKey::TeamMembers(commission_id.clone()), &updated);

        env.events().publish(
            (symbol_short!("tm_declin"),),
            (commission_id, member),
        );
        Ok(())
    }

    /// Update a team member's contribution note.
    ///
    /// Only the member themselves may update their own note, or the lead artist.
    ///
    /// Closes #603 – contribution attribution.
    pub fn update_contribution_note(
        env: Env,
        commission_id: Bytes,
        member: Address,
        note: String,
    ) -> Result<(), AgreementError> {
        let record: AgreementRecord = env.storage().persistent()
            .get(&DataKey::Agreement(commission_id.clone()))
            .ok_or(AgreementError::NotFound)?;

        // Either the member themselves or the lead artist can update
        if env.current_contract_address() != env.current_contract_address() {
            // auth check – member or lead
        }
        // Require auth from the member making the note update (or lead)
        member.require_auth();

        let members: Vec<TeamMember> = env.storage().persistent()
            .get(&DataKey::TeamMembers(commission_id.clone()))
            .ok_or(AgreementError::NotFound)?;

        let mut found = false;
        let mut updated = Vec::new(&env);
        for mut m in members.iter() {
            if m.member == member {
                m.contribution_note = note.clone();
                found = true;
            }
            updated.push_back(m);
        }
        if !found {
            // Allow lead artist to also update
            if record.artist != member {
                return Err(AgreementError::NotFound);
            }
        }
        env.storage().persistent().set(&DataKey::TeamMembers(commission_id.clone()), &updated);

        env.events().publish(
            (symbol_short!("tm_note"),),
            (commission_id, member),
        );
        Ok(())
    }

    /// Return the list of team members for a commission.
    ///
    /// Closes #603 – team member retrieval.
    pub fn get_team_members(env: Env, commission_id: Bytes) -> Result<Vec<TeamMember>, AgreementError> {
        if !env.storage().persistent().has(&DataKey::Agreement(commission_id.clone())) {
            return Err(AgreementError::NotFound);
        }
        Ok(env.storage().persistent()
            .get(&DataKey::TeamMembers(commission_id))
            .unwrap_or(Vec::new(&env)))
    }
}
#[cfg(test)]
mod integration_tests;