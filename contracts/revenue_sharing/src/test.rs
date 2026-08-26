extern crate std;
use soroban_sdk::{testutils::Address as _, token, vec, Address, Bytes, Env, String, Vec};

use crate::errors::RevenueError;
use crate::types::{AgreementStatus, Participant};
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
