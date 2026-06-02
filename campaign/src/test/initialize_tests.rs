#![cfg(test)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, Vec, String, BytesN};

use crate::types::{CampaignStatus, StellarAsset, MilestoneStatus, MilestoneData};
use crate::storage::get_campaign;
use crate::CampaignContract;

// ─── Helper: create fresh test env with contract registered ──────────────────

fn test_env() -> Env {
    let env = Env::default();
    env.register(CampaignContract, ());
    env
}

// ─── Helper: create valid XLM asset ──────────────────────────────────────────

fn xlm_asset(env: &Env) -> Vec<StellarAsset> {
    let mut assets: Vec<StellarAsset> = Vec::new(env);
    assets.push_back(StellarAsset {
        asset_code: String::from_str(env, "XLM"),
        issuer: Some(Address::generate(env)),
    });
    assets
}

// ─── Helper: create a single milestone ───────────────────────────────────────

fn single_milestone(env: &Env, goal: i128) -> Vec<MilestoneData> {
    let mut milestones: Vec<MilestoneData> = Vec::new(env);
    milestones.push_back(MilestoneData {
        index: 0,
        target_amount: goal,
        released_amount: 0,
        description_hash: BytesN::from_array(env, &[0u8; 32]),
        status: MilestoneStatus::Locked,
        released_at: None,
        released_at_ledger: None,
        release_tx: None,
        released_to: None,
    });
    milestones
}

// ─── 1. Valid initialization succeeds ─────────────────────────────────────────

#[test]
fn test_initialize_success() {
    let env = test_env();
    let creator = Address::generate(&env);
    let goal_amount: i128 = 1000;
    let end_time = env.ledger().timestamp() + 86_400;
    let assets = xlm_asset(&env);
    let milestones = single_milestone(&env, goal_amount);

    env.mock_all_auths();

    let result = CampaignContract::initialize(
        env.clone(), creator.clone(), goal_amount, end_time, assets, milestones, 0,
    );
    assert!(result.is_ok(), "Initialization should succeed");

    let campaign = get_campaign(&env).expect("Campaign should exist");
    assert_eq!(campaign.creator, creator);
    assert_eq!(campaign.goal_amount, 1000);
    assert_eq!(campaign.status, CampaignStatus::Active);
    assert_eq!(campaign.raised_amount, 0);
    assert_eq!(campaign.milestone_count, 1);
}

// ─── 2. Double initialization panics ──────────────────────────────────────────

#[test]
#[should_panic(expected = "HostError")]
fn test_initialize_double_initialization() {
    let env = test_env();
    let creator = Address::generate(&env);
    let end_time = env.ledger().timestamp() + 86_400;
    let assets = xlm_asset(&env);

    env.mock_all_auths();

    // First init
    CampaignContract::initialize(
        env.clone(), creator.clone(), 1000, end_time, assets.clone(), single_milestone(&env, 1000), 0,
    ).unwrap();

    // Second init should panic
    CampaignContract::initialize(
        env.clone(), creator.clone(), 2000, end_time + 1000, assets, single_milestone(&env, 2000), 0,
    ).unwrap();
}

// ─── 3. Zero goal_amount panics ──────────────────────────────────────────────

#[test]
#[should_panic(expected = "HostError")]
fn test_initialize_zero_goal_amount() {
    let env = test_env();
    let creator = Address::generate(&env);
    let end_time = env.ledger().timestamp() + 86_400;

    env.mock_all_auths();

    CampaignContract::initialize(
        env.clone(), creator.clone(), 0, end_time, xlm_asset(&env), single_milestone(&env, 0), 0,
    ).unwrap();
}

// ─── 4. end_time in past panics ──────────────────────────────────────────────

#[test]
#[should_panic(expected = "HostError")]
fn test_initialize_end_time_in_past() {
    let env = test_env();
    let creator = Address::generate(&env);
    // end_time 1 second ago (already passed)
    let end_time = env.ledger().timestamp() - 1;

    env.mock_all_auths();

    CampaignContract::initialize(
        env.clone(), creator.clone(), 1000, end_time, xlm_asset(&env), single_milestone(&env, 1000), 0,
    ).unwrap();
}

// ─── 5. Milestones not ascending panics ───────────────────────────────────────

#[test]
#[should_panic(expected = "HostError")]
fn test_initialize_milestones_not_ascending() {
    let env = test_env();
    let creator = Address::generate(&env);
    let end_time = env.ledger().timestamp() + 86_400;

    let mut milestones: Vec<MilestoneData> = Vec::new(&env);
    milestones.push_back(MilestoneData {
        index: 0, target_amount: 1500, released_amount: 0,
        description_hash: BytesN::from_array(&env, &[1u8; 32]),
        status: MilestoneStatus::Locked, released_at: None,
        released_at_ledger: None, release_tx: None, released_to: None,
    });
    milestones.push_back(MilestoneData {
        index: 1, target_amount: 1000, released_amount: 0,
        description_hash: BytesN::from_array(&env, &[2u8; 32]),
        status: MilestoneStatus::Locked, released_at: None,
        released_at_ledger: None, release_tx: None, released_to: None,
    });

    env.mock_all_auths();

    CampaignContract::initialize(
        env.clone(), creator.clone(), 2000, end_time, xlm_asset(&env), milestones, 0,
    ).unwrap();
}

// ─── 6. Final milestone != goal panics ────────────────────────────────────────

#[test]
#[should_panic(expected = "HostError")]
fn test_initialize_final_milestone_not_equal_goal() {
    let env = test_env();
    let creator = Address::generate(&env);
    let end_time = env.ledger().timestamp() + 86_400;

    env.mock_all_auths();

    // Milestone has target 1000 but goal is 2000
    CampaignContract::initialize(
        env.clone(), creator.clone(), 2000, end_time, xlm_asset(&env), single_milestone(&env, 1000), 0,
    ).unwrap();
}

// ─── Additional: Empty milestones panics ──────────────────────────────────────

#[test]
#[should_panic(expected = "HostError")]
fn test_initialize_empty_milestones() {
    let env = test_env();
    let creator = Address::generate(&env);
    let end_time = env.ledger().timestamp() + 86_400;
    let milestones: Vec<MilestoneData> = Vec::new(&env);

    env.mock_all_auths();

    CampaignContract::initialize(
        env.clone(), creator.clone(), 1000, end_time, xlm_asset(&env), milestones, 0,
    ).unwrap();
}

// ─── Additional: Too many milestones panics ──────────────────────────────────

#[test]
#[should_panic(expected = "HostError")]
fn test_initialize_too_many_milestones() {
    let env = test_env();
    let creator = Address::generate(&env);
    let end_time = env.ledger().timestamp() + 86_400;

    let mut milestones: Vec<MilestoneData> = Vec::new(&env);
    for i in 0..6 {
        milestones.push_back(MilestoneData {
            index: i, target_amount: (i as i128 + 1) * 1000, released_amount: 0,
            description_hash: BytesN::from_array(&env, &[(i + 1) as u8; 32]),
            status: MilestoneStatus::Locked, released_at: None,
            released_at_ledger: None, release_tx: None, released_to: None,
        });
    }

    env.mock_all_auths();

    CampaignContract::initialize(
        env.clone(), creator.clone(), 6000, end_time, xlm_asset(&env), milestones, 0,
    ).unwrap();
}

// ─── Additional: Empty assets panics ─────────────────────────────────────────

#[test]
#[should_panic(expected = "HostError")]
fn test_initialize_empty_assets() {
    let env = test_env();
    let creator = Address::generate(&env);
    let end_time = env.ledger().timestamp() + 86_400;
    let assets: Vec<StellarAsset> = Vec::new(&env);

    env.mock_all_auths();

    CampaignContract::initialize(
        env.clone(), creator.clone(), 1000, end_time, assets, single_milestone(&env, 1000), 0,
    ).unwrap();
}

// ─── Additional: Invalid asset code (empty) ──────────────────────────────────

#[test]
#[should_panic(expected = "HostError")]
fn test_initialize_empty_asset_code() {
    let env = test_env();
    let creator = Address::generate(&env);
    let end_time = env.ledger().timestamp() + 86_400;

    let mut assets: Vec<StellarAsset> = Vec::new(&env);
    assets.push_back(StellarAsset {
        asset_code: String::from_str(&env, ""),
        issuer: Some(Address::generate(&env)),
    });

    env.mock_all_auths();

    CampaignContract::initialize(
        env.clone(), creator.clone(), 1000, end_time, assets, single_milestone(&env, 1000), 0,
    ).unwrap();
}

// ─── Additional: Asset code too long (> 12 chars) ────────────────────────────

/// Validates Issue #245 — asset codes > 12 chars rejected.
#[test]
#[should_panic(expected = "HostError")]
fn test_initialize_asset_code_too_long() {
    let env = test_env();
    let creator = Address::generate(&env);
    let end_time = env.ledger().timestamp() + 86_400;

    let mut assets: Vec<StellarAsset> = Vec::new(&env);
    assets.push_back(StellarAsset {
        asset_code: String::from_str(&env, "VERYLONGASSETCODE"),
        issuer: Some(Address::generate(&env)),
    });

    env.mock_all_auths();

    CampaignContract::initialize(
        env.clone(), creator.clone(), 1000, end_time, assets, single_milestone(&env, 1000), 0,
    ).unwrap();
}

// ─── Additional: No auth panics ──────────────────────────────────────────────

#[test]
#[should_panic(expected = "HostError")]
fn test_initialize_no_auth() {
    let env = test_env();
    let creator = Address::generate(&env);
    let end_time = env.ledger().timestamp() + 86_400;

    // NOT calling mock_all_auths
    CampaignContract::initialize(
        env.clone(), creator.clone(), 1000, end_time, xlm_asset(&env), single_milestone(&env, 1000), 0,
    ).unwrap();
}
