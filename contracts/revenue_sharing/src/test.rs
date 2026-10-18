extern crate std;
use soroban_sdk::{
    testutils::Address as _, testutils::Events as _, token, vec, Address, Bytes, Env, String, Vec,
};

use crate::errors::RevenueError;
use crate::types::{AgreementStatus, Participant, TOTAL_BPS};
use crate::{RevenueSharing, RevenueSharingClient};

const HISTORY_LIMIT: u32 = 3;

struct Fixture<'a> {
    env: Env,
    client: RevenueSharingClient<'a>,
    token: Address,
    platform: Address,
    artist: Address,
    collaborator: Address,
    source: Address,
    id: Bytes,
}

fn setup<'a>() -> Fixture<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let platform = Address::generate(&env);
    let artist = Address::generate(&env);
    let collaborator = Address::generate(&env);
    let source = Address::generate(&env);

    token::StellarAssetClient::new(&env, &token).mint(&source, &1_000_000);

    let contract_id = env.register_contract(None, RevenueSharing);
    let client = RevenueSharingClient::new(&env, &contract_id);
    client.initialize(&admin, &HISTORY_LIMIT);

    Fixture {
        id: Bytes::from_slice(&env, b"track-001"),
        env,
        client,
        token,
        platform,
        artist,
        collaborator,
        source,
    }
}

impl Fixture<'_> {
    fn splits(&self, artist_bps: u32, platform_bps: u32) -> Vec<Participant> {
        vec![
            &self.env,
            Participant {
                account: self.artist.clone(),
                share_bps: artist_bps,
            },
            Participant {
                account: self.platform.clone(),
                share_bps: platform_bps,
            },
        ]
    }

    fn create(&self) {
        self.client.create_agreement(
            &self.id,
            &self.artist,
            &self.token,
            &self.splits(7000, 3000),
        );
    }

    fn balance(&self, account: &Address) -> i128 {
        token::Client::new(&self.env, &self.token).balance(account)
    }

    fn memo(&self) -> String {
        String::from_str(&self.env, "streaming payout")
    }
}

// ── Bounded reads (#984) ─────────────────────────────────────────────────────

#[test]
fn history_page_clamps_the_limit_and_pages_past_the_end() {
    let f = setup();
    f.create();

    // No payouts yet: the history is empty, and every clamp path agrees.
    let (page, info) = f.client.get_history_page(&f.id, &0, &0);
    assert!(page.is_empty());
    assert_eq!(info.total, 0);
    assert!(!info.has_more);
    assert_eq!(info.next_start, None);

    // A limit above `MAX_LIMIT` is capped, not rejected.
    let (page, _) = f.client.get_history_page(&f.id, &0, &u32::MAX);
    assert!(page.is_empty());

    // A cursor past the end is an empty final page.
    let (page, info) = f.client.get_history_page(&f.id, &99, &10);
    assert!(page.is_empty());
    assert!(!info.has_more);
    assert_eq!(info.next_start, None);
}

#[test]
fn create_agreement_stores_terms() {
    let f = setup();
    f.create();
    let agreement = f.client.get_agreement(&f.id);
    assert_eq!(agreement.owner, f.artist);
    assert_eq!(agreement.terms_version, 1);
    assert_eq!(agreement.status, AgreementStatus::Active);
    assert_eq!(f.client.get_splits(&f.id).len(), 2);
}

#[test]
fn splits_must_total_ten_thousand_bps() {
    let f = setup();
    let err = f
        .client
        .try_create_agreement(&f.id, &f.artist, &f.token, &f.splits(7000, 2000))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RevenueError::InvalidSplitTotal);
}

#[test]
fn duplicate_participants_are_rejected() {
    let f = setup();
    let participants = vec![
        &f.env,
        Participant {
            account: f.artist.clone(),
            share_bps: 5000,
        },
        Participant {
            account: f.artist.clone(),
            share_bps: 5000,
        },
    ];
    let err = f
        .client
        .try_create_agreement(&f.id, &f.artist, &f.token, &participants)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RevenueError::DuplicateParticipant);
}

#[test]
fn empty_split_is_rejected() {
    let f = setup();
    let err = f
        .client
        .try_create_agreement(&f.id, &f.artist, &f.token, &Vec::new(&f.env))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RevenueError::EmptySplit);
}

#[test]
fn duplicate_agreement_id_is_rejected() {
    let f = setup();
    f.create();
    let err = f
        .client
        .try_create_agreement(&f.id, &f.artist, &f.token, &f.splits(7000, 3000))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RevenueError::AgreementExists);
}

#[test]
fn revenue_is_distributed_by_percentage() {
    let f = setup();
    f.create();
    f.client.record_revenue(&f.id, &f.source, &1000, &f.memo());

    assert_eq!(f.balance(&f.artist), 700);
    assert_eq!(f.balance(&f.platform), 300);
    assert_eq!(f.client.get_earnings(&f.id, &f.artist), 700);
    assert_eq!(f.client.get_earnings(&f.id, &f.platform), 300);

    let report = f.client.get_report(&f.id);
    assert_eq!(report.total_revenue, 1000);
    assert_eq!(report.total_distributed, 1000);
    assert_eq!(report.entry_count, 1);
}

#[test]
fn rounding_dust_goes_to_the_first_participant() {
    let f = setup();
    f.create();
    // 7 * 7000 / 10000 = 4 (floor), 7 * 3000 / 10000 = 2; 1 unit of dust.
    f.client.record_revenue(&f.id, &f.source, &7, &f.memo());
    assert_eq!(f.balance(&f.artist), 5);
    assert_eq!(f.balance(&f.platform), 2);
    assert_eq!(f.client.get_report(&f.id).total_distributed, 7);
}

#[test]
fn earnings_accumulate_across_entries() {
    let f = setup();
    f.create();
    f.client.record_revenue(&f.id, &f.source, &1000, &f.memo());
    f.client.record_revenue(&f.id, &f.source, &500, &f.memo());
    assert_eq!(f.client.get_earnings(&f.id, &f.artist), 1050);
    assert_eq!(f.client.get_earnings(&f.id, &f.platform), 450);
    assert_eq!(f.client.get_report(&f.id).total_revenue, 1500);
}

#[test]
fn updated_terms_apply_to_later_revenue_only() {
    let f = setup();
    f.create();
    f.client.record_revenue(&f.id, &f.source, &1000, &f.memo());

    let new_terms = vec![
        &f.env,
        Participant {
            account: f.artist.clone(),
            share_bps: 5000,
        },
        Participant {
            account: f.collaborator.clone(),
            share_bps: 5000,
        },
    ];
    assert_eq!(f.client.update_splits(&f.id, &new_terms), 2);

    f.client.record_revenue(&f.id, &f.source, &1000, &f.memo());
    assert_eq!(f.client.get_earnings(&f.id, &f.artist), 1200);
    assert_eq!(f.client.get_earnings(&f.id, &f.platform), 300);
    assert_eq!(f.client.get_earnings(&f.id, &f.collaborator), 500);

    let history = f.client.get_history(&f.id);
    assert_eq!(history.len(), 2);
    assert_eq!(history.get(0).unwrap().terms_version, 1);
    assert_eq!(history.get(1).unwrap().terms_version, 2);
}

#[test]
fn paused_agreement_rejects_revenue() {
    let f = setup();
    f.create();
    f.client.set_status(&f.id, &AgreementStatus::Paused);
    let err = f
        .client
        .try_record_revenue(&f.id, &f.source, &1000, &f.memo())
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RevenueError::AgreementNotActive);
}

#[test]
fn terminated_agreement_cannot_be_reopened() {
    let f = setup();
    f.create();
    f.client.set_status(&f.id, &AgreementStatus::Terminated);
    let err = f
        .client
        .try_set_status(&f.id, &AgreementStatus::Active)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RevenueError::AgreementNotActive);
    let err = f
        .client
        .try_update_splits(&f.id, &f.splits(5000, 5000))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RevenueError::AgreementNotActive);
}

#[test]
fn non_positive_revenue_is_rejected() {
    let f = setup();
    f.create();
    let err = f
        .client
        .try_record_revenue(&f.id, &f.source, &0, &f.memo())
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RevenueError::InvalidAmount);
}

#[test]
fn unknown_agreement_is_reported() {
    let f = setup();
    let err = f
        .client
        .try_get_agreement(&Bytes::from_slice(&f.env, b"missing"))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RevenueError::AgreementNotFound);
}

#[test]
fn history_is_capped_at_the_configured_limit() {
    let f = setup();
    f.create();
    for _ in 0..HISTORY_LIMIT + 2 {
        f.client.record_revenue(&f.id, &f.source, &100, &f.memo());
    }
    let history = f.client.get_history(&f.id);
    assert_eq!(history.len(), HISTORY_LIMIT);
    // The oldest entries are dropped, so the window ends on the latest entry.
    assert_eq!(
        history.get(HISTORY_LIMIT - 1).unwrap().sequence,
        HISTORY_LIMIT + 2
    );
    assert_eq!(f.client.get_report(&f.id).entry_count, HISTORY_LIMIT + 2);
}

// ── #989: deterministic event budget ─────────────────────────────────────────

/// `record_revenue` publishes a deterministic number of contract events: one
/// `revenue` event of its own, plus one `transfer` event from the Stellar Asset
/// Contract for every positive payout. Under an equal split every recipient
/// receives a positive payout, so the budget is exactly `participants + 1`.
///
/// This mirrors the `budget_for` table in
/// `benches/revenue_sharing_benchmark.rs`. The benchmark gates on it when
/// `REVENUE_SHARING_BENCH_CHECK=1`; this test makes a change that breaks it fail
/// under a plain `cargo test`, with no benchmark run needed.
#[test]
fn event_budget_scales_with_participants() {
    for participants in [2u32, 5, 20] {
        let f = setup();
        let id = Bytes::from_slice(&f.env, b"budget");
        f.client.create_agreement(
            &id,
            &f.artist,
            &f.token,
            &equal_splits(&f.env, participants),
        );

        let events_before = f.env.events().all().len();
        f.client.record_revenue(&id, &f.source, &100_000, &f.memo());
        let events = (f.env.events().all().len() - events_before) as u32;
        assert_eq!(
            events,
            participants + 1,
            "one revenue event + one transfer event per recipient"
        );
    }
}

/// A batch that exhausts its source partway through must abort atomically: the
/// error is surfaced, and no earnings, totals or history entry are committed
/// for the half-distributed batch. This is the failure path the benchmark
/// measures (`record_revenue/insufficient`).
#[test]
fn insufficient_balance_mid_batch_is_atomic() {
    let f = setup();
    let participants = 20u32;
    let id = Bytes::from_slice(&f.env, b"mid-batch");
    f.client.create_agreement(
        &id,
        &f.artist,
        &f.token,
        &equal_splits(&f.env, participants),
    );

    // Fund a fresh source for only half the batch, so the transfer loop runs
    // out of funds partway through.
    let poor = Address::generate(&f.env);
    let per_payout = 100_000i128 / participants as i128;
    let funded = per_payout * (participants as i128 / 2);
    token::StellarAssetClient::new(&f.env, &f.token).mint(&poor, &funded);

    let result = f.client.try_record_revenue(&id, &poor, &100_000, &f.memo());
    assert!(
        !matches!(&result, Ok(Ok(_))),
        "a mid-batch shortfall must abort, not succeed"
    );

    // Atomicity: nothing was committed.
    let report = f.client.get_report(&id);
    assert_eq!(report.total_revenue, 0);
    assert_eq!(report.total_distributed, 0);
    assert_eq!(report.entry_count, 0);
    assert_eq!(f.client.get_history(&id).len(), 0);
    assert_eq!(f.balance(&poor), funded, "no partial transfer left the source");
}

/// An equal split of `participants` recipients that sums to `TOTAL_BPS`.
fn equal_splits(env: &Env, participants: u32) -> Vec<Participant> {
    let share = TOTAL_BPS / participants;
    let mut splits = Vec::new(env);
    for _ in 0..participants {
        splits.push_back(Participant {
            account: Address::generate(env),
            share_bps: share,
        });
    }
    splits
}
