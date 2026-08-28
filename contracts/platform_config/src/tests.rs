use crate::types::{FeeTier, Promotion, ReferralConfig};
use crate::PlatformConfigContractClient;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

fn setup(env: &Env) -> (crate::PlatformConfigContractClient<'_>, Address, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register_contract(None, crate::PlatformConfigContract);
    let client = PlatformConfigContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let wallet = Address::generate(env);
    let token = Address::generate(env);
    client.initialize(&admin, &500, &wallet, &token);
    (client, admin, wallet, token)
}

#[test]
fn test_set_fee_bps_success() {
    let env = Env::default();
    let (client, _, _, _) = setup(&env);
    client.set_fee_bps(&200);
    assert_eq!(client.get_config().fee_bps, 200);
}

#[test]
fn test_initialize_success() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, crate::PlatformConfigContract);
    let client = PlatformConfigContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let wallet = Address::generate(&env);
    let token = Address::generate(&env);
    client.initialize(&admin, &500, &wallet, &token);
    let config = client.get_config();
    assert_eq!(config.admin, admin);
    assert_eq!(config.fee_bps, 500);
}

#[test]
#[should_panic]
fn test_set_fee_bps_too_high() {
    let env = Env::default();
    let (client, _, _, _) = setup(&env);
    client.set_fee_bps(&1001);
}

#[test]
fn test_transfer_admin_sets_pending() {
    let env = Env::default();
    let (client, _admin, _, _) = setup(&env);
    let new_admin = Address::generate(&env);
    client.transfer_admin(&new_admin);
    client.accept_admin();
    assert_eq!(client.get_config().admin, new_admin);
}

#[test]
fn test_accept_admin_updates_admin() {
    let env = Env::default();
    let (client, _old_admin, _, _) = setup(&env);
    let new_admin = Address::generate(&env);
    client.transfer_admin(&new_admin);
    client.accept_admin();
    let config = client.get_config();
    assert_eq!(config.admin, new_admin);
}

#[test]
#[should_panic]
fn test_initialize_already_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, crate::PlatformConfigContract);
    let client = PlatformConfigContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let wallet = Address::generate(&env);
    let token = Address::generate(&env);
    client.initialize(&admin, &500, &wallet, &token);
    client.initialize(&admin, &500, &wallet, &token);
}

#[test]
#[should_panic]
fn test_initialize_fee_bps_too_high() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, crate::PlatformConfigContract);
    let client = PlatformConfigContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let wallet = Address::generate(&env);
    let token = Address::generate(&env);
    client.initialize(&admin, &1001, &wallet, &token);
}

#[test]
fn test_get_config_returns_correct_values() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, crate::PlatformConfigContract);
    let client = PlatformConfigContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let wallet = Address::generate(&env);
    let token = Address::generate(&env);
    client.initialize(&admin, &250, &wallet, &token);
    let config = client.get_config();
    assert_eq!(config.fee_bps, 250);
    assert_eq!(config.platform_wallet, wallet);
    assert_eq!(config.usdc_token, token);
}

#[test]
fn health_check_is_healthy_after_init() {
    let env = Env::default();
    let (client, admin, _, _) = setup(&env);
    let report = client.health_check();
    assert_eq!(report.status, shared::HealthStatus::Healthy);
    client.report_ok(&admin);
    assert_eq!(client.get_health_metrics().ok_count, 1);
}

// ── Advanced fee structures (#690) ─────────────────────────────────────────

#[test]
fn fee_tiers_are_exported_and_lower_the_fee() {
    let env = Env::default();
    let (client, admin, _, _) = setup(&env);

    let five_pc = FeeTier { min_volume: 0, fee_bps: 500 };
    let four_pc = FeeTier { min_volume: 50_000, fee_bps: 400 };
    assert!(!client.upsert_fee_tier(&admin, &five_pc));
    assert!(!client.upsert_fee_tier(&admin, &four_pc));
    // Replacing an existing threshold reports `true`.
    assert!(client.upsert_fee_tier(&admin, &four_pc));

    assert_eq!(client.get_fee_tiers().len(), 2);
    // Base tier below threshold volume.
    assert_eq!(client.resolve_effective_fee_bps(&10_000), 500);
    // High-volume payer gets the cheaper tier.
    assert_eq!(client.resolve_effective_fee_bps(&60_000), 400);

    // Removal works and re-list reflects it.
    assert!(client.remove_fee_tier(&admin, &50_000));
    assert_eq!(client.get_fee_tiers().len(), 1);
}

#[test]
fn volume_tracks_cumulative_and_drives_discounts() {
    let env = Env::default();
    let (client, admin, _, _) = setup(&env);
    let payer = Address::generate(&env);

    let tier = FeeTier { min_volume: 100_000, fee_bps: 300 };
    client.upsert_fee_tier(&admin, &tier);

    assert_eq!(client.get_volume(&payer), 0);
    client.record_volume(&admin, &payer, &75_000);
    client.record_volume(&admin, &payer, &75_000);
    assert_eq!(client.get_volume(&payer), 150_000);
    // 150k >= 100k -> discounted tier.
    assert_eq!(client.resolve_effective_fee_bps(&150_000), 300);
}

#[test]
fn promotion_applies_only_inside_its_window() {
    let env = Env::default();
    let (client, admin, _, _) = setup(&env);

    env.ledger().with_mut(|l| l.sequence_number = 1_000);
    let promo = Promotion { start_ledger: 1_000, end_ledger: 2_000, fee_bps: 100 };
    client.set_promotion(&admin, &promo);
    assert!(client.is_promotion_active());

    let b = client.compute_fees(&1_000_000, &0, &None);
    assert_eq!(b.effective_fee_bps, 100);
    assert_eq!(b.fee, 10_000);

    // After the window the promotion no longer applies.
    env.ledger().with_mut(|l| l.sequence_number = 2_001);
    assert!(!client.is_promotion_active());
    let b = client.compute_fees(&1_000_000, &0, &None);
    assert_eq!(b.effective_fee_bps, 500);

    client.clear_promotion(&admin);
    assert!(!client.is_promotion_active());
}

#[test]
fn referral_config_splits_the_platform_fee() {
    let env = Env::default();
    let (client, admin, referrer, _) = setup(&env);

    client.set_referral_config(&admin, &ReferralConfig { bps: 2000 });
    assert_eq!(client.get_referral_config().unwrap().bps, 2000);

    // 1_000_000 * 500bps = 50_000 fee; referrer gets 20% = 10_000.
    let b = client.compute_fees(&1_000_000, &0, &Some(referrer));
    assert_eq!(b.effective_fee_bps, 500);
    assert_eq!(b.fee, 50_000);
    assert_eq!(b.referral_fee, 10_000);
    assert_eq!(b.platform_fee, 40_000);
    assert_eq!(b.payout, 950_000);
}

#[test]
fn invalid_fee_configs_are_rejected() {
    let env = Env::default();
    let (client, admin, _, _) = setup(&env);

    let bad_tier = FeeTier { min_volume: 0, fee_bps: 1001 };
    assert_eq!(
        client.try_upsert_fee_tier(&admin, &bad_tier).err().unwrap().unwrap(),
        crate::errors::ConfigError::InvalidTier
    );

    let inverted = Promotion { start_ledger: 100, end_ledger: 50, fee_bps: 100 };
    assert_eq!(
        client.try_set_promotion(&admin, &inverted).err().unwrap().unwrap(),
        crate::errors::ConfigError::InvalidPromotion
    );

    assert_eq!(
        client.try_set_referral_config(&admin, &ReferralConfig { bps: 15_000 }).err().unwrap().unwrap(),
        crate::errors::ConfigError::InvalidReferralBps
    );
}
