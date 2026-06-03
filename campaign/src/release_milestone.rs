use soroban_sdk::{Address, Env, token, panic_with_error};
use crate::event;
use crate::types::{Error, MilestoneStatus};
use crate::storage::{get_campaign, get_milestone, set_milestone, is_frozen};
use crate::storage::{acquire_lock, get_campaign, get_milestone, release_lock, set_milestone};

/// Issue #207 – `release_milestone` function
///
/// Releases funds for an unlocked milestone to the recipient.
/// Requires creator authorization.
/// Validates milestone status is `Unlocked`.
/// Prevents double release — `Released` milestones panic with `MilestoneAlreadyReleased`.
/// Prevents skipping milestones — previous milestone must be Released.
/// Transfers tokens from contract to recipient.
/// Sets milestone status to `Released`.
/// Emits `milestone_released` event.
/// Respects the freeze flag — panics with `ContractFrozen` if frozen.
///
/// Issue #242 – Reentrancy protection: acquires lock at entry, releases at exit.
/// Issue #243 – Authorization: `creator.require_auth()`.
/// Issue #244 – Balance verification: checks contract balance before each transfer.
///
/// # Panics
/// - `Error::NotInitialized` if campaign not initialized
/// - `Error::MilestoneNotFound` if milestone index is out of range
/// - `Error::InvalidMilestoneTransition` if milestone is not `Unlocked`
/// - `Error::InsufficientContractBalance` if contract lacks funds for transfer
pub fn release_milestone(env: &Env, milestone_index: u32, recipient: Address) {
    // Issue #242 – Reentrancy protection: acquire lock
    acquire_lock(env);

    let campaign = get_campaign(env).unwrap_or_else(|| {
        panic_with_error!(env, Error::NotInitialized)
    });

    // Issue #243 – Authorization check
    campaign.creator.require_auth();

    // Freeze check — reject all mutating operations while frozen
    if is_frozen(env) {
        soroban_sdk::panic_with_error!(env, Error::ContractFrozen);
    }

    let mut milestone = get_milestone(env, milestone_index).unwrap_or_else(|| {
        panic_with_error!(env, Error::MilestoneNotFound)
    });

    // Prevent double release: milestone already in Released state
    if milestone.status == MilestoneStatus::Released {
        soroban_sdk::panic_with_error!(env, Error::MilestoneAlreadyReleased);
    }

    // Prevent releasing locked milestones (must be Unlocked first)
    if milestone.status != MilestoneStatus::Unlocked {
        panic_with_error!(env, Error::InvalidMilestoneTransition);
    }

    // Prevent skipping milestones: if not milestone 0, previous must be Released
    if milestone_index > 0 {
        let prev_milestone = get_milestone(env, milestone_index - 1).unwrap_or_else(|| {
            soroban_sdk::panic_with_error!(env, Error::MilestoneNotFound)
        });
        if prev_milestone.status != MilestoneStatus::Released {
            soroban_sdk::panic_with_error!(env, Error::PreviousMilestoneNotReleased);
        }
    }

    let release_amount = milestone
        .target_amount
        .checked_sub(milestone.released_amount)
        .unwrap_or_else(|| panic_with_error!(env, Error::Overflow));

    let timestamp = env.ledger().timestamp();

    // Transfer each accepted asset proportionally
    for asset in campaign.accepted_assets.iter() {
        if let Some(issuer) = asset.issuer.clone() {
            let token_client = token::Client::new(env, &issuer);

            // Issue #244 – Query actual contract balance for verification
            let asset_balance = token_client.balance(&env.current_contract_address());

            if asset_balance > 0 && release_amount > 0 {
                // Issue #244 – Verify contract balance is sufficient BEFORE transfer
                if asset_balance < release_amount {
                    panic_with_error!(env, Error::InsufficientContractBalance);
                }

                // Clamp to available balance (should never be needed due to check above)
                let transfer_amount = release_amount.min(asset_balance);

                token_client.transfer(&env.current_contract_address(), &recipient, &transfer_amount);

                event::milestone_released(
                    env,
                    milestone_index,
                    transfer_amount,
                    asset.asset_code.clone(),
                    &recipient,
                    timestamp,
                );
            }
        }
    }

    milestone.released_amount = milestone.target_amount;
    milestone.status = MilestoneStatus::Released;
    milestone.released_at = Some(timestamp);
    milestone.released_to = Some(recipient);
    set_milestone(env, milestone_index, &milestone);

    // Issue #242 – Release reentrancy lock
    release_lock(env);
}
