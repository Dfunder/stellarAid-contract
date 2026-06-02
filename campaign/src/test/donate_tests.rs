#![cfg(test)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, Vec, String, BytesN};

use crate::types::{CampaignStatus, AssetInfo, StellarAsset, MilestoneStatus, MilestoneData};
use crate::storage::{get_campaign, get_milestone};
use crate::CampaignContract;

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn test_env() -> Env {
    let env = Env::default();
    env.register(CampaignContract, ());
    env
}

fn xlm_asset(env: &Env) -> Vec<StellarAsset> {
    let mut assets: Vec<StellarAsset> = Vec::new(env);
    assets.push_back(StellarAsset {
        asset_code: String::from_str(env, "XLM"),
        issuer: Some(Address::generate(env)),
    });
    assets
}

fn single_milestone(env: &Env, goal: i128) -> Vec<MilestoneData> {
    let mut milestones: Vec<MilestoneData> = Vec::new(env);
    milestones.push_back(MilestoneData {
        index: 0, target_amount: goal, released_amount: 0,
        description_hash: BytesN::from_array(env, &[0u8; 32]),
        status: MilestoneStatus::Locked,
        released_at: None, released_at_ledger: None, release_tx: None, released_to: None,
    });
    milestones
}

fn multi_milestones(env: &Env, count: u32, goal: i128) -> Vec<MilestoneData> {
    let mut milestones: Vec<MilestoneData> = Vec::new(env);
    for i in 0..count {
        milestones.push_back(MilestoneData {
            index: i,
            target_amount: if i == count - 1 { goal } else { (i as i128 + 1) * 1000 },
            released_amount: 0,
            description_hash: BytesN::from_array(env, &[(i + 1) as u8; 32]),
            status: MilestoneStatus::Locked,
            released_at: None, released_at_ledger: None, release_tx: None, released_to: None,
        });
    }
    milestones
}

fn setup_minimal_campaign(env: &Env, goal: i128) -> Address {
    let creator = Address::generate(env);
    let end_time = env.ledger().timestamp() + 86_400;

    env.mock_all_auths();

    CampaignContract::initialize(
        env.clone(), creator.clone(), goal, end_time,
        xlm_asset(env), single_milestone(env, goal), 0,
    ).unwrap();
    creator
}

fn setup_campaign_with_min(
    env: &Env, goal: i128, min_donation: i128,
) -> Address {
    let creator = Address::generate(env);
    let end_time = env.ledger().timestamp() + 86_400;

    env.mock_all_auths();

    CampaignContract::initialize(
        env.clone(), creator.clone(), goal, end_time,
        xlm_asset(env), single_milestone(env, goal), min_donation,
    ).unwrap();
    creator
}

fn setup_campaign_with_milestones(
    env: &Env, goal: i128, milestones: Vec<MilestoneData>,
) -> Address {
    let creator = Address::generate(env);
    let end_time = env.ledger().timestamp() + 86_400;

    env.mock_all_auths();

    CampaignContract::initialize(
        env.clone(), creator.clone(), goal, end_time,
        xlm_asset(env), milestones, 0,
    ).unwrap();
    creator
}

// ─── 1. Valid donation updates raised_amount ─────────────────────────────────

#[test]
fn test_donate_valid_updates_raised_amount() {
    let env = test_env();
    setup_minimal_campaign(&env, 1000);

    let donor = Address::generate(&env);
    env.mock_all_auths();

    CampaignContract::donate(env.clone(), donor.clone(), 500, AssetInfo::Native);

    let campaign = get_campaign(&env).expect("Campaign should exist");
    assert_eq!(campaign.raised_amount, 500);
    assert_eq!(campaign.status, CampaignStatus::Active);

    let donor_record = CampaignContract::get_donor_record(env.clone(), donor.clone())
        .expect("Donor record should exist");
    assert_eq!(donor_record.total_donated, 500);
    assert_eq!(donor_record.donation_count, 1);
    assert!(!donor_record.refund_claimed);

    let total_raised = CampaignContract::get_total_raised(env.clone());
    assert_eq!(total_raised, 500);
}

/// Multiple donations from the same donor accumulate correctly.
#[test]
fn test_donate_multiple_from_same_donor() {
    let env = test_env();
    setup_minimal_campaign(&env, 2000);

    let donor = Address::generate(&env);
    env.mock_all_auths();

    CampaignContract::donate(env.clone(), donor.clone(), 300, AssetInfo::Native);
    CampaignContract::donate(env.clone(), donor.clone(), 700, AssetInfo::Native);

    let campaign = get_campaign(&env).expect("Campaign should exist");
    assert_eq!(campaign.raised_amount, 1000);

    let record = CampaignContract::get_donor_record(env.clone(), donor.clone())
        .expect("Donor record should exist");
    assert_eq!(record.total_donated, 1000);
    assert_eq!(record.donation_count, 2);
}

// ─── 2. Donation after deadline panics ───────────────────────────────────────

#[test]
#[should_panic(expected = "HostError")]
fn test_donate_after_deadline() {
    let env = test_env();
    let creator = Address::generate(&env);
    let end_time = env.ledger().timestamp() + 86_400;

    env.mock_all_auths();

    CampaignContract::initialize(
        env.clone(), creator.clone(), 1000, end_time,
        xlm_asset(&env), single_milestone(&env, 1000), 0,
    ).unwrap();

    // Manually set campaign to Ended state
    let mut campaign = get_campaign(&env).unwrap();
    campaign.status = CampaignStatus::Ended;
    crate::storage::set_campaign(&env, &campaign);

    let donor = Address::generate(&env);

    // Should panic: Ended status rejects donations
    CampaignContract::donate(env.clone(), donor.clone(), 100, AssetInfo::Native);
}

// ─── 3. Unaccepted asset panics ──────────────────────────────────────────────

#[test]
#[should_panic(expected = "HostError")]
fn test_donate_unaccepted_asset() {
    let env = test_env();
    setup_minimal_campaign(&env, 1000);

    let donor = Address::generate(&env);
    env.mock_all_auths();

    // Use a Stellar asset whose address is not in the accepted list
    let unknown_asset = Address::generate(&env);
    CampaignContract::donate(
        env.clone(), donor.clone(), 500,
        AssetInfo::Stellar(unknown_asset),
    );
}

// ─── 4. Zero amount panics ───────────────────────────────────────────────────

#[test]
#[should_panic(expected = "HostError")]
fn test_donate_zero_amount() {
    let env = test_env();
    setup_minimal_campaign(&env, 1000);

    let donor = Address::generate(&env);
    env.mock_all_auths();

    CampaignContract::donate(env.clone(), donor.clone(), 0, AssetInfo::Native);
}

// ─── 5. Donation below minimum panics ────────────────────────────────────────

#[test]
#[should_panic(expected = "HostError")]
fn test_donate_below_minimum() {
    let env = test_env();
    setup_campaign_with_min(&env, 1000, 100);

    let donor = Address::generate(&env);
    env.mock_all_auths();

    CampaignContract::donate(env.clone(), donor.clone(), 50, AssetInfo::Native);
}

// ─── 6. Milestone auto-unlock triggered correctly ────────────────────────────

#[test]
fn test_donate_milestone_auto_unlock() {
    let env = test_env();
    let milestones = multi_milestones(&env, 3, 3000);
    setup_campaign_with_milestones(&env, 3000, milestones);

    let donor = Address::generate(&env);
    env.mock_all_auths();

    // Donation 1: 500 — no milestone should unlock
    CampaignContract::donate(env.clone(), donor.clone(), 500, AssetInfo::Native);

    let m0 = get_milestone(&env, 0).unwrap();
    assert_eq!(m0.status, MilestoneStatus::Locked, "M0 should remain Locked at 500");

    // Donation 2: 600 — total 1100, milestone 0 unlocks (target = 1000)
    CampaignContract::donate(env.clone(), donor.clone(), 600, AssetInfo::Native);

    let m0 = get_milestone(&env, 0).unwrap();
    assert_eq!(m0.status, MilestoneStatus::Unlocked, "M0 should be Unlocked at 1100");
    let m1 = get_milestone(&env, 1).unwrap();
    assert_eq!(m1.status, MilestoneStatus::Locked, "M1 should remain Locked");

    // Donation 3: 1000 — total 2100, milestone 1 unlocks (target = 2000)
    CampaignContract::donate(env.clone(), donor.clone(), 1000, AssetInfo::Native);

    let m1 = get_milestone(&env, 1).unwrap();
    assert_eq!(m1.status, MilestoneStatus::Unlocked, "M1 should be Unlocked at 2100");
    let m2 = get_milestone(&env, 2).unwrap();
    assert_eq!(m2.status, MilestoneStatus::Locked, "M2 should remain Locked");

    // Donation 4: 900 — total 3000, milestone 2 unlocks (target = 3000)
    CampaignContract::donate(env.clone(), donor.clone(), 900, AssetInfo::Native);

    let m2 = get_milestone(&env, 2).unwrap();
    assert_eq!(m2.status, MilestoneStatus::Unlocked, "M2 should be Unlocked at 3000");
    let campaign = get_campaign(&env).unwrap();
    assert_eq!(campaign.status, CampaignStatus::GoalReached);
    assert_eq!(campaign.raised_amount, 3000);
}

/// Single large donation unlocks all milestones at once.
#[test]
fn test_donate_single_donation_unlocks_all_milestones() {
    let env = test_env();
    let milestones = multi_milestones(&env, 3, 3000);
    setup_campaign_with_milestones(&env, 3000, milestones);

    let donor = Address::generate(&env);
    env.mock_all_auths();

    CampaignContract::donate(env.clone(), donor.clone(), 3000, AssetInfo::Native);

    for i in 0..3 {
        let m = get_milestone(&env, i).unwrap();
        assert_eq!(m.status, MilestoneStatus::Unlocked, "M{} should be Unlocked", i);
    }

    let campaign = get_campaign(&env).unwrap();
    assert_eq!(campaign.status, CampaignStatus::GoalReached);
    assert_eq!(campaign.raised_amount, 3000);
}

// ─── Additional: Donate to uninitialized campaign ────────────────────────────

#[test]
#[should_panic(expected = "HostError")]
fn test_donate_uninitialized() {
    let env = test_env();
    let donor = Address::generate(&env);

    env.mock_all_auths();

    CampaignContract::donate(env.clone(), donor.clone(), 100, AssetInfo::Native);
}

// ─── Additional: Donate with GoalReached status ──────────────────────────────

#[test]
fn test_donate_when_goal_reached() {
    let env = test_env();
    setup_minimal_campaign(&env, 1000);

    let donor1 = Address::generate(&env);
    let donor2 = Address::generate(&env);
    env.mock_all_auths();

    // First donation reaches the goal
    CampaignContract::donate(env.clone(), donor1.clone(), 1000, AssetInfo::Native);

    let campaign = get_campaign(&env).unwrap();
    assert_eq!(campaign.status, CampaignStatus::GoalReached);

    // Second donation should still be accepted
    CampaignContract::donate(env.clone(), donor2.clone(), 500, AssetInfo::Native);

    let campaign = get_campaign(&env).unwrap();
    assert_eq!(campaign.raised_amount, 1500);
    assert_eq!(campaign.status, CampaignStatus::GoalReached);
}

// ─── Additional: Negative amount panics ──────────────────────────────────────

#[test]
#[should_panic(expected = "HostError")]
fn test_donate_negative_amount() {
    let env = test_env();
    setup_minimal_campaign(&env, 1000);

    let donor = Address::generate(&env);
    env.mock_all_auths();

    CampaignContract::donate(env.clone(), donor.clone(), -100, AssetInfo::Native);
}

// Note: Testing "donate without auth" requires a separate Env without mock_all_auths.
// Since setup_minimal_campaign calls mock_all_auths() and auth mocking is persistent,
// this test cannot properly verify the "no auth" scenario. Skipping this test
// as it would be a false positive (auth would still be mocked from setup).
