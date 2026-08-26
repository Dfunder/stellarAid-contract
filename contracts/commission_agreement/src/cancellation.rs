//! Cancellation reasons, policy and pro-rata settlement maths (closes #605).

use soroban_sdk::{contracttype, Address, Bytes};

use crate::errors::AgreementError;

pub const TOTAL_BPS: u32 = 10_000;

/// Default policy applied when an agreement does not set its own: a 10% penalty
/// on the walking party's share and no free-cancellation window.
pub const DEFAULT_PENALTY_BPS: u32 = 1_000;

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancellationReason {
    /// The client no longer wants the work.
    ClientRequest = 0,
    /// The artist walks away from an accepted brief.
    ArtistWithdrawal = 1,
    /// Both sides agreed to stop.
    MutualAgreement = 2,
    /// The agreement ran past its deadline without completing.
    DeadlineMissed = 3,
    /// Terms were broken; settled at completion with no penalty applied here.
    Breach = 4,
}

/// Which side, if either, carries the early-cancellation penalty.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Party {
    Neither = 0,
    Client = 1,
    Artist = 2,
}

impl CancellationReason {
    /// Only a unilateral walk-away is penalised. A mutual stop, a missed
    /// deadline or a breach settles at the completed percentage, leaving fault
    /// to the dispute process rather than pricing it here.
    pub fn penalised_party(&self) -> Party {
        match self {
            CancellationReason::ClientRequest => Party::Client,
            CancellationReason::ArtistWithdrawal => Party::Artist,
            _ => Party::Neither,
        }
    }
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CancellationPolicy {
    /// Penalty rate, in bps, charged against the share the walking party would
    /// otherwise keep.
    pub penalty_bps: u32,
    /// Ledgers after the agreement was created during which either side may
    /// cancel without penalty.
    pub grace_ledgers: u32,
}

/// The settlement an agreement would produce if cancelled right now.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CancellationQuote {
    /// Share of the budget covered by approved milestones, in bps.
    pub completion_bps: u32,
    /// Pro-rata value of the work already approved.
    pub earned: i128,
    /// Penalty moved from the walking party to the other side.
    pub penalty: i128,
    pub penalised: Party,
    /// Final amounts: these always sum to the agreement budget.
    pub artist_amount: i128,
    pub client_refund: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancellationRecord {
    pub commission_id: Bytes,
    pub initiator: Address,
    pub reason: CancellationReason,
    pub budget_usdc: i128,
    pub completion_bps: u32,
    pub penalty: i128,
    pub penalised: Party,
    pub artist_amount: i128,
    pub client_refund: i128,
    pub ledger: u32,
}

pub fn default_policy() -> CancellationPolicy {
    CancellationPolicy {
        penalty_bps: DEFAULT_PENALTY_BPS,
        grace_ledgers: 0,
    }
}

pub fn validate_policy(policy: &CancellationPolicy) -> Result<(), AgreementError> {
    if policy.penalty_bps > TOTAL_BPS {
        return Err(AgreementError::InvalidPolicy);
    }
    Ok(())
}

/// Work completion as a share of the budget, in bps, derived from the value of
/// approved milestones. Capped at 100% so an over-allocated agreement cannot
/// settle for more than its budget.
pub fn completion_bps(approved_usdc: i128, budget_usdc: i128) -> Result<u32, AgreementError> {
    if budget_usdc <= 0 {
        return Err(AgreementError::InvalidAmount);
    }
    if approved_usdc >= budget_usdc {
        return Ok(TOTAL_BPS);
    }
    if approved_usdc <= 0 {
        return Ok(0);
    }
    let scaled = approved_usdc
        .checked_mul(TOTAL_BPS as i128)
        .ok_or(AgreementError::ArithmeticOverflow)?;
    Ok((scaled / budget_usdc) as u32)
}

/// Settle a cancellation: the artist keeps the pro-rata value of approved work,
/// the client is refunded the rest, and any penalty moves across between them.
///
/// `in_grace` waives the penalty entirely. The two output amounts always sum to
/// `budget_usdc`, so the escrow can be drained exactly.
pub fn settle(
    budget_usdc: i128,
    approved_usdc: i128,
    reason: CancellationReason,
    policy: &CancellationPolicy,
    in_grace: bool,
) -> Result<CancellationQuote, AgreementError> {
    let completion = completion_bps(approved_usdc, budget_usdc)?;
    let earned = budget_usdc
        .checked_mul(completion as i128)
        .ok_or(AgreementError::ArithmeticOverflow)?
        / TOTAL_BPS as i128;
    let unearned = budget_usdc - earned;

    let penalised = if in_grace {
        Party::Neither
    } else {
        reason.penalised_party()
    };
    // The penalty is charged against whatever the walking party would have
    // walked away with: the client's refund, or the artist's earnings.
    let base = match penalised {
        Party::Client => unearned,
        Party::Artist => earned,
        Party::Neither => 0,
    };
    let penalty = base
        .checked_mul(policy.penalty_bps as i128)
        .ok_or(AgreementError::ArithmeticOverflow)?
        / TOTAL_BPS as i128;

    let artist_amount = match penalised {
        Party::Client => earned + penalty,
        Party::Artist => earned - penalty,
        Party::Neither => earned,
    };

    Ok(CancellationQuote {
        completion_bps: completion,
        earned,
        penalty,
        penalised,
        artist_amount,
        client_refund: budget_usdc - artist_amount,
    })
}
