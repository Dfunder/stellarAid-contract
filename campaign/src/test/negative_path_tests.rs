#![cfg(test)]

use soroban_sdk::testutils::Address as AddressTestUtils;
use soroban_sdk::{Address, Env, String, Vec, BytesN};

use crate::types::{
    CampaignData, CampaignStatus, DonorRecord, AssetInfo, StellarAsset, MilestoneData,
    MilestoneStatus, Error, DataKey,
};
use crate::storage::{set_campaign, set_donor, set_milestone, get_campaign};
use crate::CampaignContract;

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn make_env() -> Env {
    Env::default()
}

fn default_accepted_assets(env: &Env) -> Vec<StellarAsset> {
    let mut assets: Vec<StellarAsset> = Vec::new(env);
    assets.push_back(StellarAsset {
        asset_code: String::from_str(env, "USDC"),
        issuer: Some(Address::generate(env)),
    });
    assets
}

fn default_milestones(env: &Env) -> Vec<MilestoneData> {
    let mut milestones: Vec<MilestoneData> = Vec::new(env);
    milestones.push_back(MilestoneData {
        index: 0,
        target_amount: 1000,
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

fn initialize_default_campaign(env: &Env) -> (Address, u64) {
    let creator = Address::generate(env);
    let end_time = env.ledger().timestamp() + 100_000;
    let assets = default_accepted_assets(env);
    let milestones = default_milestones(env);
    let _ = CampaignContract::initialize(
        env.clone(),
        creator.clone(),
        1000,
        end_time,
        assets,
        milestones,
        0,
    );
    (creator, end_time)
}

fn fund_donor(env: &Env, donor: &Address) {
    let record = DonorRecord {
        donor: donor.clone(),
        total_donated: 500,
        asset: AssetInfo::Native,
        last_donation_time: env.ledger().timestamp(),
        last_donation_ledger: env.ledger().sequence(),
        donation_count: 1,
        refund_claimed: false,
    };
    set_donor(env, donor, &record);
}

fn create_donor_record(
    env: &Env,
    donor: &Address,
    total_donated: i128,
    refund_claimed: bool,
) {
    let record = DonorRecord {
        donor: donor.clone(),
        total_donated,
        asset: AssetInfo::Native,
        last_donation_time: env.ledger().timestamp(),
        last_donation_ledger: env.ledger().sequence(),
        donation_count: 1,
        refund_claimed,
    };
    set_donor(env, donor, &record);
}

// ─── Initialize negative-path tests ─────────────────────────────────────────

#[test]
#[should_panic]
fn test_initialize_fails_already_initialized() {
    let env = make_env();
    env.mock_all_auths();
    initialize_default_campaign(&env);

    // Second initialize should panic with AlreadyInitialized
    let creator = Address::generate(&env);
    let end_time = env.ledger().timestamp() + 100_000;
    let _ = CampaignContract::initialize(
        env.clone(),
        creator,
        1000,
        end_time,
        default_accepted_assets(&env),
        default_milestones(&env),
        0,
    );
}

#[test]
#[should_panic]
fn test_initialize_fails_zero_goal() {
    let env = make_env();
    env.mock_all_auths();
    let creator = Address::generate(&env);
    let end_time = env.ledger().timestamp() + 100_000;
    let _ = CampaignContract::initialize(
        env.clone(),
        creator,
        0, // Invalid: goal <= 0
        end_time,
        default_accepted_assets(&env),
        default_milestones(&env),
        0,
    );
}

#[test]
#[should_panic]
fn test_initialize_fails_negative_goal() {
    let env = make_env();
    env.mock_all_auths();
    let creator = Address::generate(&env);
    let end_time = env.ledger().timestamp() + 100_000;
    let _ = CampaignContract::initialize(
        env.clone(),
        creator,
        -100, // Invalid: negative goal
        end_time,
        default_accepted_assets(&env),
        default_milestones(&env),
        0,
    );
}

#[test]
#[should_panic]
fn test_initialize_fails_past_end_time() {
    let env = make_env();
    env.mock_all_auths();
    let creator = Address::generate(&env);
    let end_time = env.ledger().timestamp() - 1; // Past
    let _ = CampaignContract::initialize(
        env.clone(),
        creator,
        1000,
        end_time,
        default_accepted_assets(&env),
        default_milestones(&env),
        0,
    );
}

#[test]
#[should_panic]
fn test_initialize_fails_empty_assets() {
    let env = make_env();
    env.mock_all_auths();
    let creator = Address::generate(&env);
    let end_time = env.ledger().timestamp() + 100_000;
    let empty_assets: Vec<StellarAsset> = Vec::new(&env);
    let _ = CampaignContract::initialize(
        env.clone(),
        creator,
        1000,
        end_time,
        empty_assets,
        default_milestones(&env),
        0,
    );
}

#[test]
#[should_panic]
fn test_initialize_fails_empty_asset_code() {
    let env = make_env();
    env.mock_all_auths();
    let creator = Address::generate(&env);
    let end_time = env.ledger().timestamp() + 100_000;
    let mut assets: Vec<StellarAsset> = Vec::new(&env);
    assets.push_back(StellarAsset {
        asset_code: String::from_str(&env, ""), // Empty code
        issuer: Some(Address::generate(&env)),
    });
    let _ = CampaignContract::initialize(
        env.clone(),
        creator,
        1000,
        end_time,
        assets,
        default_milestones(&env),
        0,
    );
}

#[test]
#[should_panic]
fn test_initialize_fails_zero_milestones() {
    let env = make_env();
    env.mock_all_auths();
    let creator = Address::generate(&env);
    let end_time = env.ledger().timestamp() + 100_000;
    let empty_milestones: Vec<MilestoneData> = Vec::new(&env);
    let _ = CampaignContract::initialize(
        env.clone(),
        creator,
        1000,
        end_time,
        default_accepted_assets(&env),
        empty_milestones,
        0,
    );
}

#[test]
#[should_panic]
fn test_initialize_fails_too_many_milestones() {
    let env = make_env();
    env.mock_all_auths();
    let creator = Address::generate(&env);
    let end_time = env.ledger().timestamp() + 100_000;

    // Create 6 milestones (MAX_MILESTONES = 5)
    let mut milestones: Vec<MilestoneData> = Vec::new(&env);
    for i in 0..6 {
        milestones.push_back(MilestoneData {
            index: i,
            target_amount: (i as i128 + 1) * 1000,
            released_amount: 0,
            description_hash: BytesN::from_array(&env, &[0u8; 32]),
            status: MilestoneStatus::Locked,
            released_at: None,
            released_at_ledger: None,
            release_tx: None,
            released_to: None,
        });
    }

    let _ = CampaignContract::initialize(
        env.clone(),
        creator,
        6000, // goal matches last milestone target
        end_time,
        default_accepted_assets(&env),
        milestones,
        0,
    );
}

#[test]
#[should_panic]
fn test_initialize_fails_milestone_targets_not_ascending() {
    let env = make_env();
    env.mock_all_auths();
    let creator = Address::generate(&env);
    let end_time = env.ledger().timestamp() + 100_000;

    let mut milestones: Vec<MilestoneData> = Vec::new(&env);
    milestones.push_back(MilestoneData {
        index: 0,
        target_amount: 500,
        released_amount: 0,
        description_hash: BytesN::from_array(&env, &[0u8; 32]),
        status: MilestoneStatus::Locked,
        released_at: None,
        released_at_ledger: None,
        release_tx: None,
        released_to: None,
    });
    milestones.push_back(MilestoneData {
        index: 1,
        target_amount: 300, // Lower than previous — should fail
        released_amount: 0,
        description_hash: BytesN::from_array(&env, &[0u8; 32]),
        status: MilestoneStatus::Locked,
        released_at: None,
        released_at_ledger: None,
        release_tx: None,
        released_to: None,
    });

    let _ = CampaignContract::initialize(
        env.clone(),
        creator,
        500,
        end_time,
        default_accepted_assets(&env),
        milestones,
        0,
    );
}

#[test]
#[should_panic]
fn test_initialize_fails_milestone_last_target_not_equal_goal() {
    let env = make_env();
    env.mock_all_auths();
    let creator = Address::generate(&env);
    let end_time = env.ledger().timestamp() + 100_000;

    let mut milestones: Vec<MilestoneData> = Vec::new(&env);
    milestones.push_back(MilestoneData {
        index: 0,
        target_amount: 500,
        released_amount: 0,
        description_hash: BytesN::from_array(&env, &[0u8; 32]),
        status: MilestoneStatus::Locked,
        released_at: None,
        released_at_ledger: None,
        release_tx: None,
        released_to: None,
    });

    let _ = CampaignContract::initialize(
        env.clone(),
        creator,
        1000, // Goal != last milestone target (500)
        end_time,
        default_accepted_assets(&env),
        milestones,
        0,
    );
}

// ─── Donate negative-path tests ──────────────────────────────────────────────

#[test]
#[should_panic]
fn test_donate_fails_not_initialized() {
    let env = make_env();
    env.mock_all_auths();
    let donor = Address::generate(&env);
    CampaignContract::donate(env.clone(), donor, 100, AssetInfo::Native);
}

#[test]
#[should_panic]
fn test_donate_fails_campaign_ended() {
    let env = make_env();
    env.mock_all_auths();
    let (creator, _) = initialize_default_campaign(&env);

    // End the campaign first
    CampaignContract::end_campaign(env.clone());

    let donor = Address::generate(&env);
    CampaignContract::donate(env.clone(), donor, 100, AssetInfo::Native);
}

#[test]
#[should_panic]
fn test_donate_fails_campaign_cancelled() {
    let env = make_env();
    env.mock_all_auths();
    initialize_default_campaign(&env);

    // Cancel the campaign first
    CampaignContract::cancel_campaign(env.clone());

    let donor = Address::generate(&env);
    CampaignContract::donate(env.clone(), donor, 100, AssetInfo::Native);
}

#[test]
#[should_panic]
fn test_donate_fails_zero_amount() {
    let env = make_env();
    env.mock_all_auths();
    initialize_default_campaign(&env);
    let donor = Address::generate(&env);
    CampaignContract::donate(env.clone(), donor, 0, AssetInfo::Native);
}

#[test]
#[should_panic]
fn test_donate_fails_negative_amount() {
    let env = make_env();
    env.mock_all_auths();
    initialize_default_campaign(&env);
    let donor = Address::generate(&env);
    CampaignContract::donate(env.clone(), donor, -100, AssetInfo::Native);
}

#[test]
#[should_panic]
fn test_donate_fails_below_minimum() {
    let env = make_env();
    env.mock_all_auths();
    let creator = Address::generate(&env);
    let end_time = env.ledger().timestamp() + 100_000;
    let _ = CampaignContract::initialize(
        env.clone(),
        creator,
        1000,
        end_time,
        default_accepted_assets(&env),
        default_milestones(&env),
        100, // min_donation = 100
    );
    let donor = Address::generate(&env);
    CampaignContract::donate(env.clone(), donor, 50, AssetInfo::Native);
}

// ─── Refund negative-path tests ──────────────────────────────────────────────

#[test]
#[should_panic]
fn test_claim_refund_fails_not_initialized() {
    let env = make_env();
    env.mock_all_auths();
    let donor = Address::generate(&env);
    CampaignContract::claim_refund(env.clone(), donor);
}

#[test]
#[should_panic]
fn test_claim_refund_fails_no_donor_record() {
    let env = make_env();
    env.mock_all_auths();
    initialize_default_campaign(&env);
    CampaignContract::cancel_campaign(env.clone());

    let donor = Address::generate(&env);
    CampaignContract::claim_refund(env.clone(), donor);
}

#[test]
#[should_panic]
fn test_claim_refund_fails_campaign_active() {
    let env = make_env();
    env.mock_all_auths();
    initialize_default_campaign(&env);

    let donor = Address::generate(&env);
    fund_donor(&env, &donor);
    // Campaign is Active, not terminal
    CampaignContract::claim_refund(env.clone(), donor);
}

#[test]
#[should_panic]
fn test_claim_refund_fails_already_claimed() {
    let env = make_env();
    env.mock_all_auths();
    initialize_default_campaign(&env);
    CampaignContract::cancel_campaign(env.clone());

    let donor = Address::generate(&env);
    create_donor_record(&env, &donor, 500, true); // Already claimed

    CampaignContract::claim_refund(env.clone(), donor);
}

#[test]
fn test_is_refund_eligible_fails_no_campaign() {
    let env = make_env();
    let donor = Address::generate(&env);
    create_donor_record(&env, &donor, 100, false);

    let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
    assert!(!eligible, "Should not be eligible without campaign");
}

#[test]
fn test_is_refund_eligible_fails_no_donor_record() {
    let env = make_env();
    initialize_default_campaign(&env);
    CampaignContract::cancel_campaign(env.clone());

    let donor = Address::generate(&env);
    let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
    assert!(!eligible, "Non-donor should not be eligible");
}

#[test]
fn test_is_refund_eligible_fails_active_campaign() {
    let env = make_env();
    initialize_default_campaign(&env);

    let donor = Address::generate(&env);
    create_donor_record(&env, &donor, 100, false);

    let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
    assert!(!eligible, "Active campaign should not allow refunds");
}

#[test]
fn test_is_refund_eligible_fails_goal_reached() {
    let env = make_env();
    env.mock_all_auths();
    let creator = Address::generate(&env);
    let end_time = env.ledger().timestamp() + 100_000;
    let _ = CampaignContract::initialize(
        env.clone(),
        creator,
        1000,
        end_time,
        default_accepted_assets(&env),
        default_milestones(&env),
        0,
    );

    // Manually set campaign to GoalReached by setting raised >= goal
    let mut campaign = get_campaign(&env).unwrap();
    campaign.status = CampaignStatus::GoalReached;
    campaign.raised_amount = 1000;
    set_campaign(&env, &campaign);

    let donor = Address::generate(&env);
    create_donor_record(&env, &donor, 100, false);

    let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
    assert!(!eligible, "GoalReached campaign should not allow refunds");
}

#[test]
fn test_is_refund_eligible_fails_window_closed() {
    let env = make_env();
    env.mock_all_auths();
    let creator = Address::generate(&env);
    // Campaign ended more than 30 days ago
    let end_time = env.ledger().timestamp() - (31 * 24 * 60 * 60);
    let _ = CampaignContract::initialize(
        env.clone(),
        creator,
        1000,
        end_time,
        default_accepted_assets(&env),
        default_milestones(&env),
        0,
    );
    // End the campaign so it becomes terminal
    CampaignContract::end_campaign(env.clone());

    let donor = Address::generate(&env);
    create_donor_record(&env, &donor, 100, false);

    let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
    assert!(!eligible, "Refund window should be closed after 30 days");
}

#[test]
fn test_is_refund_eligible_fails_already_claimed() {
    let env = make_env();
    env.mock_all_auths();
    initialize_default_campaign(&env);
    CampaignContract::cancel_campaign(env.clone());

    let donor = Address::generate(&env);
    create_donor_record(&env, &donor, 100, true); // Already claimed

    let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
    assert!(!eligible, "Already claimed donor should not be eligible");
}

#[test]
fn test_is_refund_eligible_fails_ended_with_released_milestones() {
    let env = make_env();
    env.mock_all_auths();
    initialize_default_campaign(&env);

    // Manually set milestone to Released
    let mut milestone = crate::storage::get_milestone(&env, 0).unwrap();
    milestone.status = MilestoneStatus::Released;
    milestone.released_amount = 1000;
    set_milestone(&env, 0, &milestone);

    // End the campaign
    CampaignContract::end_campaign(env.clone());

    let donor = Address::generate(&env);
    create_donor_record(&env, &donor, 100, false);

    let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
    assert!(!eligible, "Ended campaign with released milestone should not allow refunds");
}

// ─── End campaign negative-path tests ────────────────────────────────────────

#[test]
#[should_panic]
fn test_end_campaign_fails_not_initialized() {
    let env = make_env();
    env.mock_all_auths();
    CampaignContract::end_campaign(env.clone());
}

#[test]
#[should_panic]
fn test_end_campaign_fails_already_ended() {
    let env = make_env();
    env.mock_all_auths();
    initialize_default_campaign(&env);
    CampaignContract::end_campaign(env.clone());
    // Second end should fail
    CampaignContract::end_campaign(env.clone());
}

#[test]
#[should_panic]
fn test_end_campaign_fails_cancelled() {
    let env = make_env();
    env.mock_all_auths();
    initialize_default_campaign(&env);
    CampaignContract::cancel_campaign(env.clone());
    // Can't end a cancelled campaign
    CampaignContract::end_campaign(env.clone());
}

// ─── Cancel campaign negative-path tests ─────────────────────────────────────

#[test]
#[should_panic]
fn test_cancel_campaign_fails_not_initialized() {
    let env = make_env();
    env.mock_all_auths();
    CampaignContract::cancel_campaign(env.clone());
}

#[test]
#[should_panic]
fn test_cancel_campaign_fails_already_cancelled() {
    let env = make_env();
    env.mock_all_auths();
    initialize_default_campaign(&env);
    CampaignContract::cancel_campaign(env.clone());
    // Second cancel should fail
    CampaignContract::cancel_campaign(env.clone());
}

// ─── Extend deadline negative-path tests ─────────────────────────────────────

#[test]
#[should_panic]
fn test_extend_deadline_fails_not_initialized() {
    let env = make_env();
    env.mock_all_auths();
    CampaignContract::extend_deadline(env.clone(), 999_999);
}

#[test]
#[should_panic]
fn test_extend_deadline_fails_past_time() {
    let env = make_env();
    env.mock_all_auths();
    initialize_default_campaign(&env);
    let past_time = env.ledger().timestamp() - 1;
    CampaignContract::extend_deadline(env.clone(), past_time);
}

#[test]
#[should_panic]
fn test_extend_deadline_fails_cancelled() {
    let env = make_env();
    env.mock_all_auths();
    initialize_default_campaign(&env);
    CampaignContract::cancel_campaign(env.clone());
    CampaignContract::extend_deadline(env.clone(), 999_999);
}

// ─── Reentrancy / lock tests ────────────────────────────────────────────────

#[test]
fn test_reentrancy_lock_donate_twice_succeeds() {
    // Donate should succeed sequentially (lock released after first call)
    let env = make_env();
    env.mock_all_auths();
    initialize_default_campaign(&env);
    let donor = Address::generate(&env);

    CampaignContract::donate(env.clone(), donor.clone(), 100, AssetInfo::Native);
    CampaignContract::donate(env.clone(), donor.clone(), 200, AssetInfo::Native);

    let record = CampaignContract::get_donor_record(env.clone(), donor);
    assert!(record.is_some());
    assert_eq!(record.unwrap().total_donated, 300);
}

// ─── Balance verification tests (Issue #244) ─────────────────────────────────
// These test the logical check — in real Soroban, balance queries would require
// mock token contracts. Here we test the negative path setup.

#[test]
fn test_claim_refund_eligible_cancelled() {
    // Test that a valid refund path works end-to-end (happy path)
    let env = make_env();
    env.mock_all_auths();
    initialize_default_campaign(&env);
    CampaignContract::cancel_campaign(env.clone());

    let donor = Address::generate(&env);
    create_donor_record(&env, &donor, 500, false);

    let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
    assert!(eligible, "Donor should be eligible for refund on cancelled campaign");
}

// ─── Milestone view negative-path tests ──────────────────────────────────────

#[test]
#[should_panic]
fn test_get_milestone_view_fails_not_initialized() {
    let env = make_env();
    CampaignContract::get_milestone_view(env.clone(), 0);
}

#[test]
#[should_panic]
fn test_get_milestone_view_fails_out_of_bounds() {
    let env = make_env();
    env.mock_all_auths();
    initialize_default_campaign(&env);
    CampaignContract::get_milestone_view(env.clone(), 99);
}

// ─── Edge cases ──────────────────────────────────────────────────────────────

#[test]
fn test_edge_case_zero_donations() {
    let env = make_env();
    env.mock_all_auths();
    initialize_default_campaign(&env);

    let total = CampaignContract::get_total_raised(env.clone());
    assert_eq!(total, 0, "No donations yet");
}

#[test]
fn test_edge_case_no_donor_record() {
    let env = make_env();
    initialize_default_campaign(&env);

    let stranger = Address::generate(&env);
    let record = CampaignContract::get_donor_record(env.clone(), stranger);
    assert!(record.is_none(), "Stranger should have no donor record");
}

#[test]
fn test_is_refund_eligible_returns_false_no_campaign() {
    let env = make_env();
    let donor = Address::generate(&env);
    let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
    assert!(!eligible, "Should not be eligible without any campaign");
}

#[test]
fn test_refund_window_edge_boundary() {
    let env = make_env();
    env.mock_all_auths();
    let creator = Address::generate(&env);
    // Campaign ended exactly 30 days ago (boundary)
    let end_time = env.ledger().timestamp() - (30 * 24 * 60 * 60);
    let _ = CampaignContract::initialize(
        env.clone(),
        creator,
        1000,
        end_time,
        default_accepted_assets(&env),
        default_milestones(&env),
        0,
    );
    CampaignContract::end_campaign(env.clone());

    let donor = Address::generate(&env);
    create_donor_record(&env, &donor, 100, false);

    let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
    assert!(eligible, "Should be eligible exactly at 30-day boundary");
}

#[test]
fn test_refund_window_just_after_boundary() {
    let env = make_env();
    env.mock_all_auths();
    let creator = Address::generate(&env);
    // Campaign ended 30 days + 1 second ago
    let end_time = env.ledger().timestamp() - (30 * 24 * 60 * 60 + 1);
    let _ = CampaignContract::initialize(
        env.clone(),
        creator,
        1000,
        end_time,
        default_accepted_assets(&env),
        default_milestones(&env),
        0,
    );
    CampaignContract::end_campaign(env.clone());

    let donor = Address::generate(&env);
    create_donor_record(&env, &donor, 100, false);

    let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
    assert!(!eligible, "Should NOT be eligible just past 30-day boundary");
}

// ─── Version and hello tests ─────────────────────────────────────────────────

#[test]
fn test_version() {
    let env = make_env();
    assert_eq!(CampaignContract::version(), 1);
}

#[test]
fn test_hello() {
    let env = make_env();
    let result = CampaignContract::hello(env.clone());
    assert_eq!(result, soroban_sdk::Symbol::new(&env, "campaign"));
}

// ─── Authorisation failure tests ─────────────────────────────────────────────

#[test]
#[should_panic(expected = "HostError")]
fn test_initialize_requires_auth() {
    // Without mock_all_auths, this should fail
    let env = make_env();
    let creator = Address::generate(&env);
    let end_time = env.ledger().timestamp() + 100_000;
    let _ = CampaignContract::initialize(
        env.clone(),
        creator,
        1000,
        end_time,
        default_accepted_assets(&env),
        default_milestones(&env),
        0,
    );
}

// ─── Positive-path sanity checks ─────────────────────────────────────────────

#[test]
fn test_full_lifecycle_happy_path() {
    let env = make_env();
    env.mock_all_auths();
    let (creator, _) = initialize_default_campaign(&env);

    // Check initial status
    let status = CampaignContract::get_campaign_status(env.clone());
    assert_eq!(status.status, CampaignStatus::Active);
    assert!(status.days_remaining > 0);

    // Donate
    let donor = Address::generate(&env);
    CampaignContract::donate(env.clone(), donor.clone(), 100, AssetInfo::Native);

    let total = CampaignContract::get_total_raised(env.clone());
    assert_eq!(total, 100);

    let record = CampaignContract::get_donor_record(env.clone(), donor);
    assert!(record.is_some());
}

#[test]
fn test_end_then_refund_eligible() {
    let env = make_env();
    env.mock_all_auths();
    initialize_default_campaign(&env);

    // End the campaign
    CampaignContract::end_campaign(env.clone());

    let status = CampaignContract::get_campaign_status(env.clone());
    assert_eq!(status.status, CampaignStatus::Ended);

    // Donor should be refund-eligible
    let donor = Address::generate(&env);
    create_donor_record(&env, &donor, 500, false);
    let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
    assert!(eligible);
}

#[test]
fn test_cancel_then_refund_eligible() {
    let env = make_env();
    env.mock_all_auths();
    initialize_default_campaign(&env);

    // Cancel the campaign
    CampaignContract::cancel_campaign(env.clone());

    let status = CampaignContract::get_campaign_status(env.clone());
    assert_eq!(status.status, CampaignStatus::Cancelled);

    // Donor should be refund-eligible
    let donor = Address::generate(&env);
    create_donor_record(&env, &donor, 500, false);
    let eligible = CampaignContract::is_refund_eligible(env.clone(), donor);
    assert!(eligible);
}
