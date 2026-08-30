extern crate std;

use soroban_sdk::{
    testutils::Address as _,
    Address, Bytes, Env, String,
};

use crate::{AnalyticsContract, AnalyticsContractClient, errors::AnalyticsError};

fn setup(env: &Env) -> (AnalyticsContractClient, Address) {
    let contract_id = env.register_contract(None, AnalyticsContract);
    let client = AnalyticsContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

fn make_bytes(env: &Env, s: &str) -> Bytes {
    Bytes::from_slice(env, s.as_bytes())
}

fn make_string(env: &Env, s: &str) -> String {
    String::from_str(env, s)
}

// ── Initialization ─────────────────────────────────────────────────────────

#[test]
fn test_initialize_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, _admin) = setup(&env);
    // If we reach here without panicking, initialization succeeded.
}

#[test]
fn test_double_initialize_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, AnalyticsContract);
    let client = AnalyticsContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let result = client.try_initialize(&admin);
    assert_eq!(result, Err(Ok(AnalyticsError::AlreadyInitialized)));
}

// ── Earnings recording ─────────────────────────────────────────────────────

#[test]
fn test_record_earning_increments_totals() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let client_addr = Address::generate(&env);
    let commission_id = make_bytes(&env, "comm-1");
    let category = make_string(&env, "illustration");

    client.record_earning(&artist, &commission_id, &category, &client_addr, &5_000);

    let metrics = client.get_metrics(&artist);
    assert_eq!(metrics.total_earnings, 5_000);
    assert_eq!(metrics.completed_count, 1);
    assert_eq!(metrics.cancelled_count, 0);
}

#[test]
fn test_record_earning_accumulates() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let client_addr = Address::generate(&env);
    let cat = make_string(&env, "design");

    client.record_earning(&artist, &make_bytes(&env, "c1"), &cat, &client_addr, &3_000);
    client.record_earning(&artist, &make_bytes(&env, "c2"), &cat, &client_addr, &7_000);

    let metrics = client.get_metrics(&artist);
    assert_eq!(metrics.total_earnings, 10_000);
    assert_eq!(metrics.completed_count, 2);
}

#[test]
fn test_record_earning_zero_amount_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let client_addr = Address::generate(&env);
    let cat = make_string(&env, "design");
    let result = client.try_record_earning(
        &artist, &make_bytes(&env, "c1"), &cat, &client_addr, &0,
    );
    assert_eq!(result, Err(Ok(AnalyticsError::InvalidAmount)));
}

#[test]
fn test_earning_log_stored_and_retrievable() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let client_addr = Address::generate(&env);
    let category = make_string(&env, "animation");
    let commission_id = make_bytes(&env, "anim-1");

    client.record_earning(&artist, &commission_id, &category, &client_addr, &9_000);

    assert_eq!(client.get_earning_count(&artist), 1);
    let rec = client.get_earning(&artist, &0);
    assert_eq!(rec.amount, 9_000);
    assert_eq!(rec.artist, artist);
}

// ── Cancellation ───────────────────────────────────────────────────────────

#[test]
fn test_record_cancellation_increments_count() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    client.record_cancellation(&artist);

    let metrics = client.get_metrics(&artist);
    assert_eq!(metrics.cancelled_count, 1);
    assert_eq!(metrics.completed_count, 0);
}

// ── Completion rate ────────────────────────────────────────────────────────

#[test]
fn test_completion_rate_100_percent() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let client_addr = Address::generate(&env);
    let cat = make_string(&env, "photo");

    client.record_earning(&artist, &make_bytes(&env, "p1"), &cat, &client_addr, &1_000);
    client.record_earning(&artist, &make_bytes(&env, "p2"), &cat, &client_addr, &2_000);

    assert_eq!(client.get_completion_rate(&artist), 100);
}

#[test]
fn test_completion_rate_50_percent() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let client_addr = Address::generate(&env);
    let cat = make_string(&env, "photo");

    client.record_earning(&artist, &make_bytes(&env, "p1"), &cat, &client_addr, &1_000);
    client.record_cancellation(&artist);

    assert_eq!(client.get_completion_rate(&artist), 50);
}

#[test]
fn test_completion_rate_no_data_returns_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    // artist never registered – should be NotFound
    let result = client.try_get_completion_rate(&artist);
    assert_eq!(result, Err(Ok(AnalyticsError::NotFound)));
}

// ── Response time ──────────────────────────────────────────────────────────

#[test]
fn test_record_response_time_and_avg() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    client.record_response_time(&artist, &100u64);
    client.record_response_time(&artist, &200u64);

    assert_eq!(client.get_avg_response_time(&artist), 150);
}

#[test]
fn test_record_response_time_zero_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let result = client.try_record_response_time(&artist, &0u64);
    assert_eq!(result, Err(Ok(AnalyticsError::InvalidAmount)));
}

// ── Satisfaction ───────────────────────────────────────────────────────────

#[test]
fn test_record_satisfaction_and_avg() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    client.record_satisfaction(&artist, &40u32); // 4.0
    client.record_satisfaction(&artist, &50u32); // 5.0

    assert_eq!(client.get_avg_satisfaction(&artist), 45); // 4.5 × 10
}

#[test]
fn test_satisfaction_score_too_low_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let artist = Address::generate(&env);
    let result = client.try_record_satisfaction(&artist, &5u32);
    assert_eq!(result, Err(Ok(AnalyticsError::InvalidScore)));
}

#[test]
fn test_satisfaction_score_too_high_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let artist = Address::generate(&env);
    let result = client.try_record_satisfaction(&artist, &55u32);
    assert_eq!(result, Err(Ok(AnalyticsError::InvalidScore)));
}

#[test]
fn test_satisfaction_boundary_values_ok() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let artist = Address::generate(&env);
    client.record_satisfaction(&artist, &10u32); // minimum valid
    client.record_satisfaction(&artist, &50u32); // maximum valid
    assert_eq!(client.get_avg_satisfaction(&artist), 30);
}

// ── Earnings prediction ────────────────────────────────────────────────────

#[test]
fn test_predict_earnings_average_payout() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let client_addr = Address::generate(&env);
    let cat = make_string(&env, "ui");

    client.record_earning(&artist, &make_bytes(&env, "u1"), &cat, &client_addr, &6_000);
    client.record_earning(&artist, &make_bytes(&env, "u2"), &cat, &client_addr, &4_000);

    // Mean = 10_000 / 2 = 5_000
    assert_eq!(client.predict_earnings(&artist), 5_000);
}

#[test]
fn test_predict_earnings_no_data_returns_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let result = client.try_predict_earnings(&artist);
    assert_eq!(result, Err(Ok(AnalyticsError::NotFound)));
}

// ── Not found ─────────────────────────────────────────────────────────────

#[test]
fn test_get_metrics_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let artist = Address::generate(&env);
    let result = client.try_get_metrics(&artist);
    assert_eq!(result, Err(Ok(AnalyticsError::NotFound)));
}
