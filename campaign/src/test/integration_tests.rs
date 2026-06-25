#![cfg(test)]

use soroban_sdk::testutils::Address as AddressTestUtils;
use soroban_sdk::{Address, Env, Vec, String, BytesN};

use crate::types::{CampaignStatus, CampaignData, DonorRecord, AssetInfo, StellarAsset, MilestoneStatus, MilestoneData};
use crate::storage::{get_campaign, get_milestone};
use crate::CampaignContract;

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Builds a minimal valid campaign setup.
fn setup_basic_campaign(env: &Env) -> (Address, Vec<StellarAsset>, Vec<MilestoneData>) {
    let creator = Address::generate(env);
    
    let mut assets: Vec<StellarAsset> = Vec::new(env);
    assets.push_back(StellarAsset {
        asset_code: String::from_str(env, "XLM"),
        issuer: Some(Address::generate(env)),
    });

    let mut milestones: Vec<MilestoneData> = Vec::new(env);
    milestones.push_back(MilestoneData {
        index: 0,
        target_amount: 1000,
        released_amount: 0,
        description_hash: BytesN::from_array(env, &[1u8; 32]),
        status: MilestoneStatus::Locked,
        released_at: None,
        released_at_ledger: None,
        release_tx: None,
        released_to: None,
    });

    (creator, assets, milestones)
}

// ─── Happy path: Initialize ───────────────────────────────────────────────────

/// Full campaign initialization with valid parameters should succeed.
#[test]
fn test_initialize_happy_path() {
    let env = Env::default();
    let (creator, assets, milestones) = setup_basic_campaign(&env);
    let goal_amount: i128 = 1000;
    let end_time = env.ledger().timestamp() + 86_400; // 1 day from now

    env.mock_all_auths();

    let result = CampaignContract::initialize(
        env.clone(),
        creator.clone(),
        goal_amount,
        end_time,
        assets.clone(),
        milestones.clone(),
        0,
    );

    assert!(result.is_ok(), "Initialize should succeed");

    // Verify the campaign was stored correctly
    let campaign = get_campaign(&env).expect("Campaign should exist");
    assert_eq!(campaign.creator, creator);
    assert_eq!(campaign.goal_amount, goal_amount);
    assert_eq!(campaign.status, CampaignStatus::Active);
    assert_eq!(campaign.raised_amount, 0);
    assert_eq!(campaign.milestone_count, 1);

    // Verify the milestone was stored
    let milestone = get_milestone(&env, 0).expect("Milestone should exist");
    assert_eq!(milestone.index, 0);
    assert_eq!(milestone.target_amount, 1000);
    assert_eq!(milestone.status, MilestoneStatus::Locked);
}

// ─── Happy path: Donate ───────────────────────────────────────────────────────

/// Full donation flow that reaches the goal.
#[test]
fn test_donate_happy_path() {
    let env = Env::default();
    let (creator, assets, milestones) = setup_basic_campaign(&env);
    let goal_amount: i128 = 1000;
    let end_time = env.ledger().timestamp() + 86_400;

    env.mock_all_auths();

    // Initialize campaign
    CampaignContract::initialize(
        env.clone(),
        creator.clone(),
        goal_amount,
        end_time,
        assets.clone(),
        milestones.clone(),
        0,
    ).unwrap();

    // First donation
    let donor1 = Address::generate(&env);
    CampaignContract::donate(env.clone(), donor1.clone(), 500, AssetInfo::Native);

    let campaign = get_campaign(&env).unwrap();
    assert_eq!(campaign.raised_amount, 500);
    assert_eq!(campaign.status, CampaignStatus::Active);

    // Verify donor record
    let donor_record = CampaignContract::get_donor_record(env.clone(), donor1.clone())
        .expect("Donor record should exist");
    assert_eq!(donor_record.total_donated, 500);

    // Verify total raised
    let total_raised = CampaignContract::get_total_raised(env.clone());
    assert_eq!(total_raised, 500);

    // Second donation that reaches the goal
    let donor2 = Address::generate(&env);
    CampaignContract::donate(env.clone(), donor2.clone(), 500, AssetInfo::Native);

    let campaign = get_campaign(&env).unwrap();
    assert_eq!(campaign.raised_amount, 1000);
    assert_eq!(campaign.status, CampaignStatus::GoalReached, "Campaign should transition to GoalReached");

    // Verify milestone was unlocked
    let milestone = get_milestone(&env, 0).expect("Milestone should exist");
    assert_eq!(milestone.status, MilestoneStatus::Unlocked, "Milestone should be unlocked when goal is reached");

    // Verify both donor records
    let donor1_record = CampaignContract::get_donor_record(env.clone(), donor1.clone())
        .expect("Donor 1 record should exist");
    assert_eq!(donor1_record.total_donated, 500);

    let donor2_record = CampaignContract::get_donor_record(env.clone(), donor2.clone())
        .expect("Donor 2 record should exist");
    assert_eq!(donor2_record.total_donated, 500);

    // Verify total raised
    let total_raised = CampaignContract::get_total_raised(env.clone());
    assert_eq!(total_raised, 1000);
}

// ─── Happy path: Full lifecycle with refund eligibility ───────────────────────

/// Verifies that after an Ended campaign with no released milestones, donors are refund-eligible.
#[test]
fn test_lifecycle_end_and_refund_eligibility() {
    let env = Env::default();
    let (creator, assets, milestones) = setup_basic_campaign(&env);
    let goal_amount: i128 = 1000;
    let end_time = env.ledger().timestamp() + 86_400;

    env.mock_all_auths();

    // Initialize
    CampaignContract::initialize(
        env.clone(),
        creator.clone(),
        goal_amount,
        end_time,
        assets.clone(),
        milestones.clone(),
        0,
    ).unwrap();

    // Donate
    let donor = Address::generate(&env);
    CampaignContract::donate(env.clone(), donor.clone(), 500, AssetInfo::Native);

    // Campaign is active → not refund-eligible
    assert!(
        !CampaignContract::is_refund_eligible(env.clone(), donor.clone()),
        "Donor should not be refund-eligible while campaign is active"
    );

    // After campaign ends (no milestones released), donor should be refund-eligible
    // Manually update campaign to Ended state
    let mut campaign = get_campaign(&env).unwrap();
    campaign.status = CampaignStatus::Ended;
    crate::storage::set_campaign(&env, &campaign);

    assert!(
        CampaignContract::is_refund_eligible(env.clone(), donor.clone()),
        "Donor should be refund-eligible after campaign ends with no released milestones"
    );
}

// ─── Multiple milestones lifecycle ────────────────────────────────────────────

/// Full lifecycle with 3 milestones verifying sequential unlock.
#[test]
fn test_lifecycle_multi_milestone_unlock() {
    let env = Env::default();
    let creator = Address::generate(&env);
    let goal_amount: i128 = 3000;
    let end_time = env.ledger().timestamp() + 86_400;

    let mut assets: Vec<StellarAsset> = Vec::new(&env);
    assets.push_back(StellarAsset {
        asset_code: String::from_str(&env, "XLM"),
        issuer: Some(Address::generate(&env)),
    });

    // Three milestones: 1000, 2000, 3000
    let mut milestones: Vec<MilestoneData> = Vec::new(&env);
    for i in 0..3 {
        milestones.push_back(MilestoneData {
            index: i,
            target_amount: (i as i128 + 1) * 1000,
            released_amount: 0,
            description_hash: BytesN::from_array(&env, &[(i + 1) as u8; 32]),
            status: MilestoneStatus::Locked,
            released_at: None,
            released_at_ledger: None,
            release_tx: None,
            released_to: None,
        });
    }

    env.mock_all_auths();

    // Initialize
    CampaignContract::initialize(
        env.clone(),
        creator.clone(),
        goal_amount,
        end_time,
        assets.clone(),
        milestones.clone(),
        0,
    ).unwrap();

    // First donation: 500 — no milestone should unlock yet
    let donor = Address::generate(&env);
    CampaignContract::donate(env.clone(), donor.clone(), 500, AssetInfo::Native);

    let milestone_0 = get_milestone(&env, 0).unwrap();
    assert_eq!(milestone_0.status, MilestoneStatus::Locked, "Milestone 0 should remain locked at 500 raised");

    // Second donation: 600 — total 1100, milestone 0 should unlock
    CampaignContract::donate(env.clone(), donor.clone(), 600, AssetInfo::Native);

    let milestone_0 = get_milestone(&env, 0).unwrap();
    assert_eq!(milestone_0.status, MilestoneStatus::Unlocked, "Milestone 0 should unlock at 1100 raised");
    let milestone_1 = get_milestone(&env, 1).unwrap();
    assert_eq!(milestone_1.status, MilestoneStatus::Locked, "Milestone 1 should remain locked");

    // Third donation: 1000 — total 2100, milestone 1 should unlock
    CampaignContract::donate(env.clone(), donor.clone(), 1000, AssetInfo::Native);

    let milestone_1 = get_milestone(&env, 1).unwrap();
    assert_eq!(milestone_1.status, MilestoneStatus::Unlocked, "Milestone 1 should unlock at 2100 raised");
    let milestone_2 = get_milestone(&env, 2).unwrap();
    assert_eq!(milestone_2.status, MilestoneStatus::Locked, "Milestone 2 should remain locked");

    // Fourth donation: 900 — total 3000, milestone 2 should unlock
    CampaignContract::donate(env.clone(), donor.clone(), 900, AssetInfo::Native);

    let milestone_2 = get_milestone(&env, 2).unwrap();
    assert_eq!(milestone_2.status, MilestoneStatus::Unlocked, "Milestone 2 should unlock at 3000 raised");

    // Campaign should be GoalReached
    let campaign = get_campaign(&env).unwrap();
    assert_eq!(campaign.status, CampaignStatus::GoalReached);
}

// ─── Version check ────────────────────────────────────────────────────────────

#[test]
fn test_version() {
    assert_eq!(CampaignContract::version(), 1);
}

// ─── Hello check ──────────────────────────────────────────────────────────────

#[test]
fn test_hello() {
    let env = Env::default();
    let result = CampaignContract::hello(env.clone());
    assert_eq!(result, soroban_sdk::Symbol::new(&env, "campaign"));
}

// ─── Get total raised before any donations ────────────────────────────────────

#[test]
fn test_get_total_raised_default() {
    let env = Env::default();
    let total = CampaignContract::get_total_raised(env.clone());
    assert_eq!(total, 0, "Total raised should be 0 before any donations");
}

// ─── Get donor record for non-donor ──────────────────────────────────────────

#[test]
fn test_get_donor_record_non_donor() {
    let env = Env::default();
    let non_donor = Address::generate(&env);
    let record = CampaignContract::get_donor_record(env.clone(), non_donor.clone());
    assert!(record.is_none(), "Non-donor should not have a record");
}

// ─── Donate with minimum donation amount enforced ────────────────────────────

#[test]
#[should_panic(expected = "HostError")]
fn test_donate_below_minimum_panics_assert() {
    let env = Env::default();
    let (creator, assets, milestones) = setup_basic_campaign(&env);
    let end_time = env.ledger().timestamp() + 86_400;

    env.mock_all_auths();

    CampaignContract::initialize(
        env.clone(),
        creator.clone(),
        1000,
        end_time,
        assets.clone(),
        milestones.clone(),
        100, // min donation is 100
    ).unwrap();

    let donor = Address::generate(&env);
    CampaignContract::donate(env.clone(), donor.clone(), 50, AssetInfo::Native);
}

// ─── Donate to uninitialized campaign ────────────────────────────────────────

#[test]
#[should_panic(expected = "HostError")]
fn test_donate_uninitialized() {
    let env = Env::default();
    let donor = Address::generate(&env);
    env.mock_all_auths();

    CampaignContract::donate(env.clone(), donor.clone(), 100, AssetInfo::Native);
}