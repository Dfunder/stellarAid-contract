use soroban_sdk::{contracttype, Address, Bytes, String};

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgreementStatus {
    Pending = 0,
    Active = 1,
    Completed = 2,
    Cancelled = 3,
    Disputed = 4,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MilestoneStatus {
    Pending = 0,
    Approved = 1,
    Rejected = 2,
}

/// Role a team member holds in a commission agreement.
///
/// Closes #603 – role-based access for team collaboration.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TeamRole {
    /// Full authority over the commission (typically the original artist).
    Lead = 0,
    /// Can submit work and propose milestones; cannot accept/reject agreements.
    Contributor = 1,
    /// Read-only access; no write operations permitted.
    Viewer = 2,
}

/// Invitation status for a team member.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvitationStatus {
    /// Invitation sent but not yet accepted.
    Pending = 0,
    /// Invitation accepted; member is active.
    Accepted = 1,
    /// Invitation declined.
    Declined = 2,
}

/// A team member record attached to a commission agreement.
///
/// Closes #603 – team member invitation flow and contribution attribution.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TeamMember {
    pub member: Address,
    pub role: TeamRole,
    pub invitation_status: InvitationStatus,
    /// Payment share in basis points (0–10000). Sum of all members must be ≤ 10000.
    pub payment_share_bps: u32,
    /// Description of this member's contribution.
    pub contribution_note: String,
    /// Ledger when this member was added.
    pub added_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgreementRecord {
    pub commission_id: Bytes,
    pub client: Address,
    pub artist: Address,
    pub title: String,
    pub budget_usdc: i128,
    pub deadline_ledger: u32,
    pub status: AgreementStatus,
    pub created_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneRecord {
    pub milestone_id: Bytes,
    pub commission_id: Bytes,
    pub title: String,
    pub amount_usdc: i128,
    pub status: MilestoneStatus,
}

#[contracttype]
pub enum DataKey {
    Agreement(Bytes),
    Milestone(Bytes, Bytes), // (commission_id, milestone_id)
    MilestonesForAgreement(Bytes),
    /// List of team members for a commission.  Key: commission_id.
    TeamMembers(Bytes),
}
