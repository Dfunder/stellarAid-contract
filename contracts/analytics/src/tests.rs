extern crate std;

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
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

// ── Bounded page reads (#650) ───────────────────────────────────────────────

#[test]
fn test_get_earnings_returns_bounded_page() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    let client_addr = Address::generate(&env);
    let cat = make_string(&env, "ui");

    // Distinct amounts so page ordering is observable.
    for i in 1..=5u32 {
        client.record_earning(
            &artist,
            &make_bytes(&env, &format!("c{}", i)),
            &cat,
            &client_addr,
            &(i as i128 * 100),
        );
    }

    let first = client.get_earnings(&artist, &0, &2);
    assert_eq!(first.len(), 2, "limit must bound the page size");
    assert_eq!(first.get(0).unwrap().amount, 100);
    assert_eq!(first.get(1).unwrap().amount, 200);

    let second = client.get_earnings(&artist, &2, &2);
    assert_eq!(second.len(), 2);
    assert_eq!(second.get(0).unwrap().amount, 300);
    assert_eq!(second.get(1).unwrap().amount, 400);

    // Past the end is an empty page, not an error, so a client can page to the
    // end without first reading get_earning_count.
    assert_eq!(client.get_earnings(&artist, &99, &2).len(), 0);

    // A limit of 0 is an empty page rather than an unbounded read.
    assert_eq!(client.get_earnings(&artist, &0, &0).len(), 0);
}

// ── Retention / pruning (#651) ──────────────────────────────────────────────

/// Record `n` earnings of 1_000 each for `artist`, each carrying a distinct
/// `commission_id` so a record can be looked up individually.
///
/// The caller positions the ledger first: `EarningsRecord.ledger` is stamped
/// from `env.ledger().sequence()`, and that stamp is what the retention check
/// keys off.
fn seed_earnings(env: &Env, client: &AnalyticsContractClient, artist: &Address, n: u32) {
    assert!(n > 0, "seed_earnings needs at least one record");
    let client_addr = Address::generate(env);
    let cat = make_string(env, "seed");
    for i in 0..n {
        client.record_earning(
            artist,
            &make_bytes(env, &format!("seed-{}", i)),
            &cat,
            &client_addr,
            &1_000,
        );
    }
}

/// Move the ledger forward far enough that every entry recorded at
/// `recorded_at` has aged out of the retention window.
fn age_out(env: &Env, recorded_at: u32) {
    env.ledger()
        .set_sequence_number(recorded_at.saturating_add(crate::ANALYTICS_TTL_LEDGERS));
}

/// A ledger sequence at which an entry recorded *now* is still comfortably
/// inside the retention window, so a prune at this ledger must not touch it.
///
/// The default `Env` starts at ledger 0, where `cutoff` saturates to 0 and a
/// record stamped at 0 would be immediately prunable. Every prune test
/// therefore positions the ledger explicitly first.
fn ledger_inside_window() -> u32 {
    crate::ANALYTICS_TTL_LEDGERS + 100
}

#[test]
fn test_prune_earnings_respects_retention_window() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    // The view entry point reports the same numbers the implementation uses.
    let (retention, batch) = client.get_retention_policy();
    assert_eq!(retention, crate::ANALYTICS_TTL_LEDGERS);
    assert_eq!(batch, crate::PRUNE_MAX_BATCH);

    let artist = Address::generate(&env);
    env.ledger().set_sequence_number(ledger_inside_window());
    seed_earnings(&env, &client, &artist, 3);

    // Every record is inside the window, so nothing may be removed — not even
    // with the widest window and the largest batch the implementation allows.
    assert_eq!(client.prune_earnings(&artist, &10, &batch), Ok(0));
    assert_eq!(client.get_earning_count(&artist), 3);
    assert!(client.try_get_earning(&artist, &0).is_ok());

    // Age the records out, then the same call succeeds.
    age_out(&env, ledger_inside_window());
    assert_eq!(client.prune_earnings(&artist, &10, &batch), Ok(3));

    // Records are gone, but the monotonic count is intentionally not lowered —
    // decrementing it would let a later record_earning reuse a live index.
    assert_eq!(
        client.try_get_earning(&artist, &0),
        Err(Ok(AnalyticsError::NotFound)),
    );
    assert_eq!(client.get_earning_count(&artist), 3);
}

#[test]
fn test_prune_earnings_is_bounded_by_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    env.ledger().set_sequence_number(ledger_inside_window());
    seed_earnings(&env, &client, &artist, 6);
    age_out(&env, ledger_inside_window());

    // One call removes at most `limit` records; progress is made by repetition.
    assert_eq!(client.prune_earnings(&artist, &6, &2), Ok(2));
    assert_eq!(client.prune_earnings(&artist, &6, &2), Ok(2));
    assert_eq!(client.prune_earnings(&artist, &6, &2), Ok(2));
    // Nothing left to remove.
    assert_eq!(client.prune_earnings(&artist, &6, &2), Ok(0));

    // The lifetime aggregate is never pruned.
    let metrics = client.get_metrics(&artist);
    assert_eq!(metrics.completed_count, 6);
    assert_eq!(metrics.total_earnings, 6_000);
}

#[test]
fn test_prune_earnings_rejects_out_of_range_arguments() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    env.ledger().set_sequence_number(ledger_inside_window());
    seed_earnings(&env, &client, &artist, 2);
    age_out(&env, ledger_inside_window());

    let (_retention, batch) = client.get_retention_policy();

    // A zero window is meaningless.
    assert_eq!(
        client.try_prune_earnings(&artist, &0, &10),
        Err(Ok(AnalyticsError::InvalidAmount)),
    );
    // A zero limit would be an unbounded-looking call.
    assert_eq!(
        client.try_prune_earnings(&artist, &10, &0),
        Err(Ok(AnalyticsError::InvalidAmount)),
    );
    // A limit above the batch cap is rejected outright rather than clamped, so
    // an operator asking for a huge sweep gets an error instead of a surprise.
    assert_eq!(
        client.try_prune_earnings(&artist, &10, &(batch + 1)),
        Err(Ok(AnalyticsError::InvalidAmount)),
    );
    // Rejected calls remove nothing.
    assert!(client.try_get_earning(&artist, &0).is_ok());
    assert_eq!(client.get_earning_count(&artist), 2);
}

#[test]
fn test_prune_earnings_stops_at_a_protected_record() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let artist = Address::generate(&env);
    env.ledger().set_sequence_number(ledger_inside_window());
    seed_earnings(&env, &client, &artist, 2);
    age_out(&env, ledger_inside_window());

    // One record written *after* ageing — so it is inside the retention window.
    let client_addr = Address::generate(&env);
    let cat = make_string(&env, "recent");
    client.record_earning(
        &artist,
        &make_bytes(&env, "recent"),
        &cat,
        &client_addr,
        &1_000,
    );

    // A wide window and a wide batch must still stop at the protected record
    // rather than reaching past it.
    assert_eq!(
        client.prune_earnings(&artist, &100, &100),
        Ok(2),
        "prune must stop at the retention boundary",
    );

    // The in-window record survives.
    assert!(client.try_get_earning(&artist, &2).is_ok());
    assert_eq!(client.get_earning_count(&artist), 3);
}
