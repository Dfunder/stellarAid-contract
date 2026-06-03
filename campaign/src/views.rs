//! View helpers for enriched milestone data.
//!
//! Provides `MilestoneView` and functions to compute enriched fields
//! (`pending_release`, `is_fully_released`, `is_next_pending`) that
//! are derived from the raw `MilestoneData` stored on-chain.

use soroban_sdk::{panic_with_error, Env};

use crate::storage::{get_campaign, get_milestone};
use crate::types::{Error, MilestoneData, MilestoneStatus};

// ─── Enriched Milestone View ─────────────────────────────────────────────────

/// Enriched milestone view with computed fields derived from the raw stored
/// `MilestoneData` and the current campaign state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneView {
    /// The raw milestone data from storage.
    pub data: MilestoneData,
    /// Amount pending release (`target_amount - released_amount`).
    pub pending_release: i128,
    /// Whether the milestone has been fully released (`released_amount >= target_amount`).
    pub is_fully_released: bool,
    /// Whether this milestone is the next one that should be released
    /// (all prior milestones have been released).
    pub is_next_pending: bool,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Find the index of the first milestone that is not yet released.
/// Returns `milestone_count` if all milestones are released.
pub fn find_next_pending_index(env: &Env) -> u32 {
    let campaign = get_campaign(env)
        .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized));

    for i in 0..campaign.milestone_count {
        if let Some(milestone) = get_milestone(env, i) {
            if milestone.status != MilestoneStatus::Released {
                return i;
            }
        }
    }
    campaign.milestone_count
}

/// Returns the enriched `MilestoneView` for the milestone at `index`.
///
/// # Panics
/// - `Error::NotInitialized` — contract not initialised.
/// - `Error::MilestoneNotFound` — `index` ≥ `milestone_count` or missing storage.
pub fn get_milestone_by_index(env: &Env, index: u32) -> MilestoneView {
    let campaign = get_campaign(env)
        .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized));

    if index >= campaign.milestone_count {
        panic_with_error!(env, Error::MilestoneNotFound);
    }

    let data = get_milestone(env, index)
        .unwrap_or_else(|| panic_with_error!(env, Error::MilestoneNotFound));

    let pending_release = data.pending_release();
    let is_fully_released = data.is_fully_released();
    let is_next_pending = find_next_pending_index(env) == index;

    MilestoneView {
        data,
        pending_release,
        is_fully_released,
        is_next_pending,
    }
}
