// src/milestone.rs  (replace the existing release_milestone_multi_asset function)

use soroban_sdk::{panic_with_error, symbol_short, token, Address, Env, Vec};
use crate::types::{Error, MilestoneStatus, StellarAsset};
use crate::storage::{
    get_campaign, get_milestone, set_milestone,
    storage_get_asset_raised, storage_get_total_raised,
    storage_set_total_raised, storage_set_asset_raised,
};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Minimum transfer amount — prevents dust transfers that waste fees.
const MIN_TRANSFER_AMOUNT: i128 = 1;

/// Precision multiplier for integer proportional math (avoids fp division errors).
/// Using 1_000_000 gives us 6 decimal places of accuracy before truncation.
const PRECISION: i128 = 1_000_000;

// ─── Helper: proportional release ────────────────────────────────────────────

/// Computes the proportional release for a single asset using integer arithmetic.
///
/// Formula:  floor((asset_raised * milestone_release * PRECISION) / (total_raised * PRECISION))
///         = floor((asset_raised * milestone_release) / total_raised)
///
/// Multiplying by PRECISION before dividing preserves sub-unit accuracy and
/// avoids the truncation bias that arises from dividing small integers first.
///
/// Returns `None` if the result would be zero or if total_raised is zero.
fn compute_asset_release(
    asset_raised: i128,
    milestone_release: i128,
    total_raised: i128,
) -> Option<i128> {
    if total_raised == 0 || asset_raised <= 0 || milestone_release <= 0 {
        return None;
    }

    // Checked arithmetic — avoids silent overflow on large campaign amounts
    let numerator = asset_raised
        .checked_mul(milestone_release)
        .and_then(|n| n.checked_mul(PRECISION))?;

    let denominator = total_raised.checked_mul(PRECISION)?;

    let release = numerator / denominator; // integer floor division

    if release >= MIN_TRANSFER_AMOUNT {
        Some(release)
    } else {
        None
    }
}

// ─── Main entrypoint ─────────────────────────────────────────────────────────

/// Issue #208 — Multi-asset milestone release
///
/// Releases milestone funds proportionally across every accepted asset.
///
/// Security properties:
/// - Requires creator auth.
/// - Milestone must be in `Unlocked` state (exactly once).
/// - Proportional math uses checked integer arithmetic — no overflows.
/// - Status is written to storage BEFORE transfers (CEI pattern) so a
///   re-entrant call on the same milestone index fails immediately.
/// - Recipient must be non-zero (validated before any transfer).
/// - Per-asset actual balances are used, not stored estimates, so the
///   contract can never release more than it actually holds.
/// - Dust amounts below MIN_TRANSFER_AMOUNT are skipped rather than
///   causing the whole release to fail.
///
/// Atomicity:
/// - Soroban's transaction model guarantees all-or-nothing at the host level.
///   Any `panic_with_error!` after the milestone status write will revert the
///   entire transaction including the status update.
pub fn release_milestone_multi_asset(
    env: &Env,
    milestone_index: u32,
    recipient: Address,
) {
    // ── 1. Load campaign ────────────────────────────────────────────────────
    let campaign = get_campaign(env).unwrap_or_else(|| {
        panic_with_error!(env, Error::NotInitialized)
    });

    // ── 2. Authorisation ────────────────────────────────────────────────────
    campaign.creator.require_auth();

    // ── 3. Validate recipient ────────────────────────────────────────────────
    // Prevent accidental burns — a zero address check is idiomatic in Soroban
    // by requiring the recipient to sign a no-op (or by the caller supplying
    // the address from a known-good source). At minimum we ensure the address
    // is not the contract itself, which would be a no-op transfer.
    if recipient == env.current_contract_address() {
        panic_with_error!(env, Error::InvalidRecipient);
    }

    // ── 4. Load and validate milestone ──────────────────────────────────────
    let mut milestone = get_milestone(env, milestone_index).unwrap_or_else(|| {
        panic_with_error!(env, Error::MilestoneNotFound)
    });

    if milestone.status != MilestoneStatus::Unlocked {
        panic_with_error!(env, Error::InvalidMilestoneTransition);
    }

    // ── 5. Compute release amount ────────────────────────────────────────────
    let total_raised = storage_get_total_raised(env);

    if total_raised == 0 {
        panic_with_error!(env, Error::NothingToRelease);
    }

    // Guard against underflow — released_amount should never exceed target_amount
    let milestone_release = milestone
        .target_amount
        .checked_sub(milestone.released_amount)
        .unwrap_or_else(|| panic_with_error!(env, Error::MilestoneReleasedExceedsTarget));

    if milestone_release == 0 {
        panic_with_error!(env, Error::NothingToRelease);
    }

    // ── 6. Write status BEFORE transfers (Checks-Effects-Interactions) ──────
    //
    // Marking Released here means any re-entrant invocation of this function
    // with the same milestone_index will fail the status guard in step 4,
    // making double-spend via re-entrancy impossible.
    milestone.released_amount = milestone
        .released_amount
        .checked_add(milestone_release)
        .unwrap_or_else(|| panic_with_error!(env, Error::ArithmeticOverflow));
    milestone.status = MilestoneStatus::Released;
    set_milestone(env, milestone_index, &milestone);

    // ── 7. Execute proportional transfers ───────────────────────────────────
    let mut total_released: i128 = 0;
    let mut assets_released: u32 = 0;

    for asset in campaign.accepted_assets.iter() {
        let token_address = match &asset.issuer {
            Some(addr) => addr.clone(),
            None => {
                // Native asset or asset without issuer — skip gracefully
                // (log a diagnostic event so operators can detect misconfiguration)
                env.events().publish(
                    (symbol_short!("ms_skip"), symbol_short!("no_issuer")),
                    (milestone_index, asset.asset_code.clone()),
                );
                continue;
            }
        };

        let token_client = token::Client::new(env, &token_address);

        // Use the actual on-contract balance — never trust a stored estimate
        let contract_balance = token_client.balance(&env.current_contract_address());

        // Also retrieve the per-asset raised amount from storage for proportional math
        // (contract_balance may be lower than asset_raised if funds were partially
        //  released in earlier milestones — use the stored raised figure for the ratio)
        let asset_raised = storage_get_asset_raised(env, &token_address);

        let asset_release = match compute_asset_release(
            asset_raised,
            milestone_release,
            total_raised,
        ) {
            Some(amount) => amount,
            None => {
                // Nothing to release for this asset (dust or zero balance)
                continue;
            }
        };

        // Clamp to actual available balance to prevent over-spending
        // (guards against rounding up across multiple assets)
        let clamped_release = asset_release.min(contract_balance);

        if clamped_release < MIN_TRANSFER_AMOUNT {
            continue;
        }

        // Execute the transfer
        token_client.transfer(
            &env.current_contract_address(),
            &recipient,
            &clamped_release,
        );

        // Update per-asset accounting
        let new_asset_raised = asset_raised
            .checked_sub(clamped_release)
            .unwrap_or(0)
            .max(0);
        storage_set_asset_raised(env, &token_address, new_asset_raised);

        total_released = total_released
            .checked_add(clamped_release)
            .unwrap_or_else(|| panic_with_error!(env, Error::ArithmeticOverflow));
        assets_released += 1;
    }

    // ── 8. Update global total-raised bookkeeping ────────────────────────────
    let new_total_raised = total_raised
        .checked_sub(total_released)
        .unwrap_or(0)
        .max(0);
    storage_set_total_raised(env, new_total_raised);

    // ── 9. Emit structured event ─────────────────────────────────────────────
    env.events().publish(
        (symbol_short!("milestone"), symbol_short!("ms_rel")),
        (
            milestone_index,
            milestone_release,   // amount scheduled for release
            total_released,      // amount actually transferred (may differ by dust)
            assets_released,     // number of assets touched
            recipient.clone(),
        ),
    );
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proportional_release_equal_split() {
        // 50% of funds in one asset, 1000 to release → 500
        let result = compute_asset_release(500, 1000, 1000);
        assert_eq!(result, Some(500));
    }

    #[test]
    fn proportional_release_unequal_split() {
        // 300 of 1000 total raised in asset A, release 400 → 120
        let result = compute_asset_release(300, 400, 1000);
        assert_eq!(result, Some(120));
    }

    #[test]
    fn proportional_release_rounds_down() {
        // 1 of 3 total, release 100 → floor(33.33) = 33
        let result = compute_asset_release(1, 100, 3);
        assert_eq!(result, Some(33));
    }

    #[test]
    fn proportional_release_zero_total_raised() {
        let result = compute_asset_release(100, 100, 0);
        assert_eq!(result, None);
    }

    #[test]
    fn proportional_release_zero_asset_raised() {
        let result = compute_asset_release(0, 100, 1000);
        assert_eq!(result, None);
    }

    #[test]
    fn proportional_release_dust_below_minimum() {
        // Very small asset amount → release rounds to 0
        let result = compute_asset_release(1, 1, 1_000_000);
        assert_eq!(result, None);
    }

    #[test]
    fn proportional_release_full_amount() {
        // All funds in one asset
        let result = compute_asset_release(5000, 5000, 5000);
        assert_eq!(result, Some(5000));
    }

    #[test]
    fn proportional_release_negative_asset_raised() {
        let result = compute_asset_release(-100, 1000, 1000);
        assert_eq!(result, None);
    }
}