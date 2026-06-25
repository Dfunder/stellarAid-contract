#![no_std]

pub mod contract;
pub mod event;
pub mod get_all_milestones;
pub mod get_milestone;
pub mod multi_asset_release;
pub mod release_milestone;
pub mod storage;
pub mod types;
pub mod views;

use soroban_sdk::{contract, contractimpl, panic_with_error, token, Address, BytesN, Env, String, Vec};
use storage::{
    acquire_lock, get_admin, get_campaign, get_campaign_or_panic, get_donor,
    get_donor_asset_donation, get_milestone, increment_donor_asset_donation, is_frozen,
    release_lock, set_admin, set_campaign, set_donor, set_frozen, set_milestone,
    storage_get_asset_raised, storage_increment_asset_raised, storage_get_total_raised,
    storage_set_total_raised,
};
use types::{
    AssetInfo, CampaignData, CampaignInitializedEvent, CampaignStatus, CampaignStatusResponse,
    DonorRecord, Error, MilestoneData, MilestoneStatus, StellarAsset,
};

pub const VERSION: u32 = 1;

/// Refund window duration: 30 days in seconds.
pub const REFUND_WINDOW: u64 = 30 * 24 * 60 * 60;

#[contract]
pub struct CampaignContract;

#[contractimpl]
impl CampaignContract {
    /// Initialize a new campaign.
    ///
    /// Also sets the admin to `creator` — admin can later be rotated via `set_admin`.
    pub fn initialize(
        env: Env,
        creator: Address,
        goal_amount: i128,
        end_time: u64,
        accepted_assets: Vec<StellarAsset>,
        milestones: Vec<MilestoneData>,
        min_donation_amount: i128,
    ) -> Result<(), Error> {
        creator.require_auth();

        if get_campaign(&env).is_some() {
            panic_with_error!(&env, Error::AlreadyInitialized);
        }

        if goal_amount <= 0 {
            panic_with_error!(&env, Error::InvalidGoalAmount);
        }

        let current_timestamp = env.ledger().timestamp();
        if end_time <= current_timestamp {
            panic_with_error!(&env, Error::InvalidEndTime);
        }

        if accepted_assets.is_empty() {
            panic_with_error!(&env, Error::InvalidAssets);
        }

        validate_assets(&env, &accepted_assets)?;

        let milestone_count = milestones.len() as u32;
        if milestone_count == 0 || milestone_count > types::MAX_MILESTONES {
            panic_with_error!(&env, Error::InvalidMilestoneCount);
        }

        validate_milestones(&env, &milestones, goal_amount)?;

        let campaign = CampaignData {
            creator: creator.clone(),
            goal_amount,
            raised_amount: 0,
            end_time,
            status: CampaignStatus::Active,
            accepted_assets: accepted_assets.clone(),
            milestone_count,
            min_donation_amount,
            created_at_ledger: env.ledger().sequence(),
            created_at_time: env.ledger().timestamp(),
            concluded_at_ledger: None,
        };

        set_campaign(&env, &campaign);
        // Issue #270 – admin defaults to creator; rotatable via set_admin
        set_admin(&env, &creator);

        for (index, milestone) in milestones.iter().enumerate() {
            set_milestone(&env, index as u32, &milestone);
        }

        env.events().publish(
            ("campaign", "initialized"),
            CampaignInitializedEvent {
                creator,
                goal_amount,
                end_time,
                asset_count: accepted_assets.len() as u32,
                milestone_count,
                created_at_ledger: env.ledger().sequence(),
            },
        );

        Ok(())
    }

    /// Issue #270 – Transfer admin rights to a new address.
    ///
    /// Both the current admin AND the new admin must authorize this call,
    /// ensuring the new admin accepts the role explicitly.
    ///
    /// # Panics
    /// - `Error::NotInitialized` if contract not initialized
    /// - `Error::Unauthorized` if caller is not the current admin
    pub fn set_admin(env: Env, new_admin: Address) {
        let current_admin = get_admin(&env)
            .unwrap_or_else(|| panic_with_error!(&env, Error::NotInitialized));

        // Current admin must authorize
        current_admin.require_auth();
        // New admin must accept the role
        new_admin.require_auth();

        set_admin(&env, &new_admin);

        event::admin_changed(&env, &current_admin, &new_admin);
    }

    /// Issue #269 – Freeze the contract, blocking all state-changing operations.
    ///
    /// # Panics
    /// - `Error::NotInitialized` if contract not initialized
    /// - `Error::Unauthorized` if caller is not the admin
    pub fn freeze(env: Env) {
        let admin = get_admin(&env)
            .unwrap_or_else(|| panic_with_error!(&env, Error::NotInitialized));
        admin.require_auth();

        set_frozen(&env, true);

        event::contract_frozen(&env, &admin, env.ledger().timestamp());
    }

    /// Issue #269 – Unfreeze the contract, re-enabling state-changing operations.
    ///
    /// # Panics
    /// - `Error::NotInitialized` if contract not initialized
    /// - `Error::Unauthorized` if caller is not the admin
    pub fn unfreeze(env: Env) {
        let admin = get_admin(&env)
            .unwrap_or_else(|| panic_with_error!(&env, Error::NotInitialized));
        admin.require_auth();

        set_frozen(&env, false);

        event::contract_unfrozen(&env, &admin, env.ledger().timestamp());
    }

    /// Issue #268 – Upgrade the contract WASM in-place.
    ///
    /// Uses `env.deployer().update_current_contract_wasm(new_wasm_hash)` so
    /// all on-chain state is preserved across the upgrade.
    ///
    /// # Panics
    /// - `Error::NotInitialized` if contract not initialized
    /// - `Error::Unauthorized` if caller is not the admin
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        let admin = get_admin(&env)
            .unwrap_or_else(|| panic_with_error!(&env, Error::NotInitialized));
        admin.require_auth();

        env.deployer().update_current_contract_wasm(new_wasm_hash.clone());

        event::contract_upgraded(&env, &admin, new_wasm_hash, env.ledger().timestamp());
    }

    /// Donate to the campaign.
    pub fn donate(env: Env, donor: Address, amount: i128, asset: AssetInfo) {
        acquire_lock(&env);
        donor.require_auth();

        if is_frozen(&env) {
            panic_with_error!(&env, Error::ContractFrozen);
        }

        let mut campaign: CampaignData = get_campaign(&env)
            .unwrap_or_else(|| panic_with_error!(&env, Error::NotInitialized));

        match campaign.status {
            CampaignStatus::Active | CampaignStatus::GoalReached => {}
            _ => panic_with_error!(&env, Error::CampaignNotActive),
        }

        if amount <= 0 || (campaign.min_donation_amount > 0 && amount < campaign.min_donation_amount) {
            panic_with_error!(&env, Error::DonationTooSmall);
        }

        campaign.raised_amount = campaign
            .raised_amount
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, Error::Overflow));

        if campaign.raised_amount >= campaign.goal_amount
            && campaign.status == CampaignStatus::Active
        {
            campaign.status = CampaignStatus::GoalReached;
            env.events().publish(
                ("campaign", "campaign_goal_reached"),
                campaign.raised_amount,
            );
        }

        set_campaign(&env, &campaign);

        let new_total = storage_get_total_raised(&env)
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, Error::Overflow));
        storage_set_total_raised(&env, new_total);

        let asset_address = get_token_address_for_asset(&env, &asset, &campaign);
        increment_donor_asset_donation(&env, &donor, &asset_address, amount);
        storage_increment_asset_raised(&env, &asset_address, amount);

        let mut donor_record = get_donor(&env, &donor)
            .unwrap_or_else(|| DonorRecord::new_for(&env, donor.clone()));
        donor_record.apply_donation(
            &env,
            amount,
            env.ledger().timestamp(),
            env.ledger().sequence(),
            asset.clone(),
        );
        set_donor(&env, &donor, &donor_record);

        for i in 0..campaign.milestone_count {
            if let Some(mut milestone) = get_milestone(&env, i) {
                if milestone.status == MilestoneStatus::Locked
                    && campaign.raised_amount >= milestone.target_amount
                {
                    milestone.status = MilestoneStatus::Unlocked;
                    set_milestone(&env, i, &milestone);
                    event::milestone_unlocked(&env, i, milestone.target_amount, campaign.raised_amount);
                }
            }
        }

        let asset_code = resolve_asset_code(&env, &asset, &campaign);
        event::donation_received(
            &env,
            &donor,
            amount,
            asset_code,
            campaign.raised_amount,
            env.ledger().timestamp(),
        );

        release_lock(&env);
    }

    /// Returns the total amount raised by the campaign.
    pub fn get_total_raised(env: Env) -> i128 {
        storage_get_total_raised(&env)
    }

    /// Returns the amount raised per accepted asset.
    pub fn get_raised_per_asset(env: Env) -> Vec<(AssetInfo, i128)> {
        let campaign = get_campaign_or_panic(&env);
        let mut result = Vec::new(&env);
        for asset in campaign.accepted_assets.iter() {
            let asset_info = if asset.is_xlm() {
                AssetInfo::Native
            } else {
                AssetInfo::Stellar(asset.issuer.clone().unwrap())
            };
            let token_address = get_token_address_for_asset(&env, &asset_info, &campaign);
            let amount = storage_get_asset_raised(&env, &token_address);
            result.push_back((asset_info, amount));
        }
        result
    }

    /// Returns the donor record for the given address.
    pub fn get_donor_record(env: Env, donor: Address) -> Option<DonorRecord> {
        get_donor(&env, &donor)
    }

    /// Returns the asset-specific breakdown of a donor's contributions.
    pub fn get_donor_asset_breakdown(env: Env, donor: Address) -> Vec<(AssetInfo, i128)> {
        get_donor(&env, &donor)
            .map(|record| record.contributions)
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn hello(env: Env) -> soroban_sdk::Symbol {
        soroban_sdk::Symbol::new(&env, "campaign")
    }

    pub fn version() -> u32 {
        VERSION
    }

    /// Returns the current admin address.
    pub fn get_admin(env: Env) -> Option<Address> {
        get_admin(&env)
    }

    /// Check if a donor is eligible to claim a refund.
    pub fn is_refund_eligible(env: Env, donor: Address) -> bool {
        let campaign = match get_campaign(&env) {
            Some(c) => c,
            None => return false,
        };

        let donor_record = match get_donor(&env, &donor) {
            Some(d) => d,
            None => return false,
        };

        if !campaign.status.is_terminal() {
            return false;
        }

        match campaign.status {
            CampaignStatus::Cancelled => {}
            CampaignStatus::Ended => {
                for i in 0..campaign.milestone_count {
                    if let Some(milestone) = get_milestone(&env, i) {
                        if milestone.status == MilestoneStatus::Released {
                            return false;
                        }
                    }
                }
            }
            _ => return false,
        }

        let current_time = env.ledger().timestamp();
        if current_time > campaign.end_time + REFUND_WINDOW {
            return false;
        }

        if donor_record.refund_claimed {
            return false;
        }

        true
    }

    /// Claim a refund for a donation.
    pub fn claim_refund(env: Env, donor: Address) {
        acquire_lock(&env);
        donor.require_auth();

        if is_frozen(&env) {
            panic_with_error!(&env, Error::ContractFrozen);
        }

        let campaign = get_campaign(&env)
            .unwrap_or_else(|| panic_with_error!(&env, Error::NotInitialized));

        let mut donor_record = get_donor(&env, &donor)
            .unwrap_or_else(|| panic_with_error!(&env, Error::NoDonorRecord));

        if !campaign.status.is_terminal() {
            panic_with_error!(&env, Error::RefundNotPermitted);
        }

        match campaign.status {
            CampaignStatus::Cancelled => {}
            CampaignStatus::Ended => {
                for i in 0..campaign.milestone_count {
                    if let Some(milestone) = get_milestone(&env, i) {
                        if milestone.status == MilestoneStatus::Released {
                            panic_with_error!(&env, Error::RefundNotPermitted);
                        }
                    }
                }
            }
            _ => panic_with_error!(&env, Error::RefundNotPermitted),
        }

        let current_time = env.ledger().timestamp();
        if current_time > campaign.end_time + REFUND_WINDOW {
            panic_with_error!(&env, Error::RefundWindowClosed);
        }

        if donor_record.refund_claimed {
            panic_with_error!(&env, Error::RefundAlreadyClaimed);
        }

        let mut total_released: i128 = 0;
        for i in 0..campaign.milestone_count {
            if let Some(milestone) = get_milestone(&env, i) {
                total_released += milestone.released_amount;
            }
        }

        let refund_numerator = campaign.raised_amount - total_released;
        let refund_denominator = campaign.raised_amount;

        donor_record.refund_claimed = true;
        set_donor(&env, &donor, &donor_record);

        for i in 0..donor_record.contributions.len() {
            if let Some((asset, donor_asset_amount)) = donor_record.contributions.get(i) {
                if donor_asset_amount > 0 {
                    let refund_amount =
                        (donor_asset_amount * refund_numerator) / refund_denominator;

                    if refund_amount > 0 {
                        let asset_address =
                            get_token_address_for_asset(&env, &asset, &campaign);
                        let token_client = token::Client::new(&env, &asset_address);
                        let contract_balance =
                            token_client.balance(&env.current_contract_address());
                        if contract_balance < refund_amount {
                            panic_with_error!(&env, Error::InsufficientContractBalance);
                        }
                        token_client.transfer(
                            &env.current_contract_address(),
                            &donor,
                            &refund_amount,
                        );
                        env.events().publish(
                            ("campaign", "asset_refund"),
                            (donor.clone(), asset_address, refund_amount),
                        );
                    }
                }
            }
        }

        env.events().publish(
            ("campaign", "refund_claimed"),
            (&donor, donor_record.total_donated),
        );

        release_lock(&env);
    }

    /// End the campaign early.
    pub fn end_campaign(env: Env) {
        contract::end_campaign(&env);
    }

    /// Cancel the campaign.
    pub fn cancel_campaign(env: Env) {
        contract::cancel_campaign(&env);
    }

    /// Extend the campaign deadline.
    pub fn extend_deadline(env: Env, new_end_time: u64) {
        contract::extend_deadline(&env, new_end_time);
    }

    /// Get campaign status with computed fields.
    pub fn get_campaign_status(env: Env) -> CampaignStatusResponse {
        contract::get_campaign_status(&env)
    }

    /// Release a single milestone (all assets proportionally).
    pub fn release_milestone(env: Env, milestone_index: u32, recipient: Address) {
        release_milestone::release_milestone(&env, milestone_index, recipient);
    }

    /// Multi-asset milestone release with proportional distribution.
    pub fn release_milestone_multi_asset(env: Env, milestone_index: u32, recipient: Address) {
        multi_asset_release::release_milestone_multi_asset(&env, milestone_index, recipient);
    }

    /// Get milestone view (raw data).
    pub fn get_milestone_view(env: Env, index: u32) -> MilestoneData {
        get_milestone::get_milestone_view(&env, index)
    }

    /// Get all milestones (enriched views).
    pub fn get_all_milestones(env: Env) -> Vec<views::MilestoneView> {
        get_all_milestones::get_all_milestones_view(&env)
    }
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

fn get_token_address_for_asset(env: &Env, asset: &AssetInfo, campaign: &CampaignData) -> Address {
    match asset {
        AssetInfo::Stellar(addr) => {
            if !campaign
                .accepted_assets
                .iter()
                .any(|a| a.issuer.as_ref() == Some(addr))
            {
                panic_with_error!(env, Error::AssetNotAccepted);
            }
            addr.clone()
        }
        AssetInfo::Native => {
            let xlm_code = String::from_str(env, "XLM");
            campaign
                .accepted_assets
                .iter()
                .find(|a| a.asset_code == xlm_code)
                .and_then(|a| a.issuer.clone())
                .unwrap_or_else(|| panic_with_error!(env, Error::AssetNotAccepted))
        }
    }
}

fn resolve_asset_code(env: &Env, asset: &AssetInfo, campaign: &CampaignData) -> String {
    match asset {
        AssetInfo::Native => String::from_str(env, "XLM"),
        AssetInfo::Stellar(addr) => campaign
            .accepted_assets
            .iter()
            .find(|a| a.issuer.as_ref() == Some(addr))
            .map(|a| a.asset_code.clone())
            .unwrap_or_else(|| String::from_str(env, "UNKNOWN")),
    }
}

fn validate_assets(env: &Env, assets: &Vec<StellarAsset>) -> Result<(), Error> {
    for asset in assets.iter() {
        if !asset.has_valid_code() {
            panic_with_error!(env, Error::InvalidAssetCode);
        }
    }
    Ok(())
}

fn validate_milestones(
    env: &Env,
    milestones: &Vec<MilestoneData>,
    goal_amount: i128,
) -> Result<(), Error> {
    let mut last_target = 0i128;
    for (i, milestone) in milestones.iter().enumerate() {
        if milestone.target_amount <= last_target {
            panic_with_error!(env, Error::InvalidMilestones);
        }
        last_target = milestone.target_amount;
        if i == (milestones.len() - 1) as usize && milestone.target_amount != goal_amount {
            panic_with_error!(env, Error::MilestoneMismatch);
        }
    }
    Ok(())
}

pub fn validate_campaign_transition(
    env: &Env,
    current_status: &CampaignStatus,
    next_status: &CampaignStatus,
) -> Result<(), Error> {
    if current_status.can_transition_to(*next_status) {
        Ok(())
    } else {
        panic_with_error!(env, Error::InvalidCampaignTransition)
    }
}

pub fn validate_milestone_transition(
    env: &Env,
    current_status: &MilestoneStatus,
    next_status: &MilestoneStatus,
) -> Result<(), Error> {
    if current_status.can_transition_to(*next_status) {
        Ok(())
    } else {
        panic_with_error!(env, Error::InvalidMilestoneTransition)
    }
}

#[cfg(test)]
mod test {
    pub mod claim_refund_tests;
    pub mod get_campaign_status_tests;
    pub mod integration_tests;
    pub mod negative_path_tests;
    pub mod refund_eligibility_tests;
    pub mod release_milestone_tests;
    pub mod admin_tests;
    pub mod formal_verification_tests;
}