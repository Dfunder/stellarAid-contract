//! Portfolio analytics contract tests — closes #602.

#![cfg(test)]

extern crate std;

use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, String};

use crate::{errors::AnalyticsError, AnalyticsContract, AnalyticsContractClient};

fn make_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env
}

fn make_str(env: &Env, s: &str) -> String {
    String::from_str(env, s)
}

fn make_bytes(env: &Env, s: &str) -> Bytes {
    Bytes::from_slice(env, s.as_bytes())
}

struct Setup {
    client: AnalyticsContractClient<'static>,
    admin: Address,
    artist: Address,
}

fn setup(env: &Env) -> Setup {
    let contract_id = env.register_contract(None, AnalyticsContract);
    let client = AnalyticsContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let artist = Address::generate(env);

    client.initialize(&admin).unwrap();
    Setup { client, admin, artist }
}

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

#[test]
fn initialize_once() {
    let env = make_env();
    let s = setup(&env);

    // Second init must fail.
    let err = s.client.initialize(&s.admin).unwrap_err();
    assert_eq!(err, AnalyticsError::AlreadyInitialized);
}

// ---------------------------------------------------------------------------
// Project lifecycle
// ---------------------------------------------------------------------------

#[test]
fn record_started_and_completed() {
    let env = make_env();
    let s = setup(&env);

    s.client.record_project_started(&s.artist).unwrap();
    s.client
        .record_project_completed(
            &s.artist,
            &50_000i128,
            &make_str(&env, "illustration"),
            &make_bytes(&env, "comm001"),
        )
        .unwrap();

    let stats = s.client.get_artist_stats(&s.artist);
    assert_eq!(stats.projects_started, 1);
    assert_eq!(stats.projects_completed, 1);
    assert_eq!(stats.total_earnings, 50_000);
}

#[test]
fn record_cancelled_increments_cancelled_count() {
    let env = make_env();
    let s = setup(&env);

    s.client.record_project_started(&s.artist).unwrap();
    s.client.record_project_cancelled(&s.artist).unwrap();

    let stats = s.client.get_artist_stats(&s.artist);
    assert_eq!(stats.projects_cancelled, 1);
}

#[test]
fn completion_rate_100_percent() {
    let env = make_env();
    let s = setup(&env);

    s.client.record_project_started(&s.artist).unwrap();
    s.client
        .record_project_completed(
            &s.artist,
            &10_000i128,
            &make_str(&env, "ui"),
            &make_bytes(&env, "c1"),
        )
        .unwrap();

    let cs = s.client.get_completion_stats(&s.artist);
    assert_eq!(cs.completion_rate_bps, 10_000);
}

#[test]
fn completion_rate_50_percent() {
    let env = make_env();
    let s = setup(&env);

    s.client.record_project_started(&s.artist).unwrap();
    s.client.record_project_started(&s.artist).unwrap();
    s.client
        .record_project_completed(
            &s.artist,
            &20_000i128,
            &make_str(&env, "design"),
            &make_bytes(&env, "c2"),
        )
        .unwrap();

    let cs = s.client.get_completion_stats(&s.artist);
    assert_eq!(cs.completion_rate_bps, 5_000); // 50%
}

// ---------------------------------------------------------------------------
// Category earnings
// ---------------------------------------------------------------------------

#[test]
fn category_earnings_tracked_separately() {
    let env = make_env();
    let s = setup(&env);

    s.client
        .record_project_completed(
            &s.artist,
            &30_000i128,
            &make_str(&env, "illustration"),
            &make_bytes(&env, "c1"),
        )
        .unwrap();
    s.client
        .record_project_completed(
            &s.artist,
            &20_000i128,
            &make_str(&env, "animation"),
            &make_bytes(&env, "c2"),
        )
        .unwrap();
    s.client
        .record_project_completed(
            &s.artist,
            &10_000i128,
            &make_str(&env, "illustration"),
            &make_bytes(&env, "c3"),
        )
        .unwrap();

    let illus = s
        .client
        .get_category_earnings(&s.artist, &make_str(&env, "illustration"));
    assert_eq!(illus.earnings, 40_000);
    assert_eq!(illus.project_count, 2);

    let anim = s
        .client
        .get_category_earnings(&s.artist, &make_str(&env, "animation"));
    assert_eq!(anim.earnings, 20_000);
    assert_eq!(anim.project_count, 1);
}

#[test]
fn artist_categories_list_is_deduplicated() {
    let env = make_env();
    let s = setup(&env);

    for _ in 0..3 {
        s.client
            .record_project_completed(
                &s.artist,
                &10_000i128,
                &make_str(&env, "illustration"),
                &make_bytes(&env, "cx"),
            )
            .unwrap();
    }

    let cats = s.client.get_artist_categories(&s.artist);
    assert_eq!(cats.len(), 1); // deduplicated
}

// ---------------------------------------------------------------------------
// Response time analytics
// ---------------------------------------------------------------------------

#[test]
fn avg_response_time_computed() {
    let env = make_env();
    let s = setup(&env);

    s.client
        .record_response_time(&s.artist, &100u32)
        .unwrap();
    s.client
        .record_response_time(&s.artist, &200u32)
        .unwrap();

    let rt = s.client.get_response_time_stats(&s.artist);
    assert_eq!(rt.sample_count, 2);
    assert_eq!(rt.avg_response_time_ledgers, 150); // (100+200)/2
}

// ---------------------------------------------------------------------------
// Client satisfaction trends
// ---------------------------------------------------------------------------

#[test]
fn satisfaction_trend_records_datapoints() {
    let env = make_env();
    let s = setup(&env);

    s.client
        .record_satisfaction(&s.artist, &make_bytes(&env, "c1"), &80u32)
        .unwrap();
    s.client
        .record_satisfaction(&s.artist, &make_bytes(&env, "c2"), &60u32)
        .unwrap();

    let trend = s.client.get_satisfaction_trend(&s.artist);
    assert_eq!(trend.len(), 2);
}

#[test]
fn avg_satisfaction_computed() {
    let env = make_env();
    let s = setup(&env);

    s.client
        .record_satisfaction(&s.artist, &make_bytes(&env, "c1"), &80u32)
        .unwrap();
    s.client
        .record_satisfaction(&s.artist, &make_bytes(&env, "c2"), &60u32)
        .unwrap();

    let avg = s.client.get_avg_satisfaction(&s.artist);
    assert_eq!(avg, 70);
}

#[test]
fn satisfaction_out_of_range_rejected() {
    let env = make_env();
    let s = setup(&env);

    let err = s.client
        .record_satisfaction(&s.artist, &make_bytes(&env, "c1"), &0u32)
        .unwrap_err();
    assert_eq!(err, AnalyticsError::InvalidValue);

    let err2 = s.client
        .record_satisfaction(&s.artist, &make_bytes(&env, "c1"), &101u32)
        .unwrap_err();
    assert_eq!(err2, AnalyticsError::InvalidValue);
}

// ---------------------------------------------------------------------------
// Earnings predictions
// ---------------------------------------------------------------------------

#[test]
fn earnings_prediction_computed() {
    let env = make_env();
    let s = setup(&env);

    env.ledger().set_sequence_number(1_000_000);

    s.client
        .record_project_completed(
            &s.artist,
            &100_000i128,
            &make_str(&env, "design"),
            &make_bytes(&env, "c1"),
        )
        .unwrap();

    // first_recorded_ledger = 0 → ~2 months active at ledger 1_000_000
    s.client
        .compute_earnings_prediction(&s.artist, &0u32)
        .unwrap();

    let pred = s.client.get_earnings_prediction(&s.artist).unwrap();
    assert!(pred.predicted_monthly_earnings > 0);
    assert!(pred.months_active >= 2);
}

// ---------------------------------------------------------------------------
// Error codes
// ---------------------------------------------------------------------------

#[test]
fn error_codes_are_stable() {
    assert_eq!(AnalyticsError::NotInitialized as u32, 1);
    assert_eq!(AnalyticsError::AlreadyInitialized as u32, 2);
    assert_eq!(AnalyticsError::NotFound as u32, 3);
    assert_eq!(AnalyticsError::Unauthorized as u32, 4);
    assert_eq!(AnalyticsError::InvalidValue as u32, 5);
    assert_eq!(AnalyticsError::ArithmeticOverflow as u32, 6);
    assert_eq!(AnalyticsError::CategoryTooLong as u32, 7);
}
