//! Commission revision requests: proposed changes to the work, with a
//! deadline, an optional budget adjustment, and a bounded, tracked history
//! (closes #600).
//!
//! A revision never moves any tokens itself — it only adjusts the agreed
//! `budget_usdc` figure on the `AgreementRecord` once accepted. Actual fund
//! custody/movement lives outside this contract (see the escrow contract);
//! this keeps revision negotiation a pure bookkeeping change to the agreed
//! terms, the same way `propose_milestone` only ever records amounts.

use soroban_sdk::{contracttype, Address, Bytes, String};

use crate::errors::AgreementError;

/// Revisions requested on an agreement are capped at this count unless the
/// client sets a different limit via `set_revision_policy` (while the
/// agreement is still `Pending`).
pub const DEFAULT_MAX_REVISIONS: u32 = 5;

/// Upper bound `set_revision_policy` accepts, so a limit can't be set so high
/// it defeats the point of having one.
pub const MAX_REVISIONS_CAP: u32 = 50;

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RevisionStatus {
    Pending = 0,
    Accepted = 1,
    Rejected = 2,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevisionRequest {
    pub commission_id: Bytes,
    /// Whoever proposed the change — the artist proposing revised work, or
    /// the client requesting changes; either may call `request_revision`.
    pub requester: Address,
    pub description: String,
    /// Ledger by which the revision must be resolved (informational — it is
    /// not itself enforced as a hard cutoff here, since resolution requires
    /// the other party's action; callers can use it to detect a stale
    /// request via `get_revisions`).
    pub deadline_ledger: u32,
    /// Proposed change to `AgreementRecord.budget_usdc`; positive is
    /// additional cost, negative is a discount. Applied only on acceptance.
    pub cost_adjustment: i128,
    pub status: RevisionStatus,
    pub requested_ledger: u32,
    pub resolved_ledger: u32,
    pub response_note: Option<String>,
}

#[contracttype]
pub struct RevisionPolicy {
    pub max_revisions: u32,
}

pub fn default_policy() -> RevisionPolicy {
    RevisionPolicy {
        max_revisions: DEFAULT_MAX_REVISIONS,
    }
}

pub fn validate_policy(policy: &RevisionPolicy) -> Result<(), AgreementError> {
    if policy.max_revisions == 0 || policy.max_revisions > MAX_REVISIONS_CAP {
        return Err(AgreementError::InvalidPolicy);
    }
    Ok(())
}
