//! Agency roster, revenue split and batch distribution tests (#609).

extern crate std;
use soroban_sdk::{testutils::Address as _, token, vec, Address, Bytes, Env, String, Vec};

use crate::agency::BatchPayment;
use crate::errors::AgreementError;
use crate::{CommissionAgreementContract, CommissionAgreementContractClient};

const DEFAULT_SPLIT: u32 = 2_000;

struct Fixture<'a> {
    env: Env,
    client_api: CommissionAgreementContractClient<'a>,
    token: Address,
    agency: Address,
    artist: Address,
    other_artist: Address,
    commissioner: Address,
}

fn setup<'a>() -> Fixture<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let agency = Address::generate(&env);
    token::StellarAssetClient::new(&env, &token).mint(&agency, &1_000_000);

    let contract_id = env.register_contract(None, CommissionAgreementContract);
    let client_api = CommissionAgreementContractClient::new(&env, &contract_id);

    Fixture {
        artist: Address::generate(&env),
        other_artist: Address::generate(&env),
        commissioner: Address::generate(&env),
        env,
        client_api,
        token,
        agency,
    }
}

impl Fixture<'_> {
    fn register(&self) {
        self.client_api.register_agency(
            &self.agency,
            &String::from_str(&self.env, "Northlight Studio"),
            &DEFAULT_SPLIT,
        );
    }

    fn register_with_roster(&self) {
        self.register();
        self.client_api
            .add_artist(&self.agency, &self.artist, &DEFAULT_SPLIT);
        self.client_api
            .add_artist(&self.agency, &self.other_artist, &1_000);
    }

    fn balance(&self, account: &Address) -> i128 {
        token::Client::new(&self.env, &self.token).balance(account)
    }

    fn batch(&self, lines: Vec<BatchPayment>) -> i128 {
        self.client_api
            .distribute_batch(&self.agency, &self.token, &lines)
    }
}

#[test]
fn register_agency_creates_a_profile() {
    let f = setup();
    f.register();
    let profile = f.client_api.get_agency(&f.agency);
    assert_eq!(profile.agency, f.agency);
    assert_eq!(profile.default_split_bps, DEFAULT_SPLIT);
    assert_eq!(profile.artist_count, 0);
    assert!(f.client_api.get_roster(&f.agency).is_empty());
}

#[test]
fn duplicate_agency_registration_is_rejected() {
    let f = setup();
    f.register();
    let err = f
        .client_api
        .try_register_agency(
            &f.agency,
            &String::from_str(&f.env, "Northlight Studio"),
            &DEFAULT_SPLIT,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AgreementError::AgencyExists);
}

#[test]
fn split_bps_is_bounded() {
    let f = setup();
    let err = f
        .client_api
        .try_register_agency(&f.agency, &String::from_str(&f.env, "Bad"), &10_001)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AgreementError::InvalidSplitBps);

    f.register();
    let err = f
        .client_api
        .try_add_artist(&f.agency, &f.artist, &10_001)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AgreementError::InvalidSplitBps);
}

#[test]
fn roster_management_tracks_membership() {
    let f = setup();
    f.register_with_roster();

    assert_eq!(f.client_api.get_agency(&f.agency).artist_count, 2);
    assert_eq!(f.client_api.get_roster(&f.agency).len(), 2);
    assert_eq!(
        f.client_api.get_artist_agency(&f.artist),
        Some(f.agency.clone())
    );

    f.client_api.remove_artist(&f.agency, &f.artist);
    assert_eq!(f.client_api.get_agency(&f.agency).artist_count, 1);
    assert_eq!(f.client_api.get_roster(&f.agency).len(), 1);
    assert_eq!(f.client_api.get_artist_agency(&f.artist), None);
}

#[test]
fn an_artist_can_only_be_represented_once() {
    let f = setup();
    f.register_with_roster();
    let err = f
        .client_api
        .try_add_artist(&f.agency, &f.artist, &1_000)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AgreementError::ArtistAlreadyRepresented);
}

#[test]
fn actions_on_an_unregistered_agency_are_rejected() {
    let f = setup();
    let err = f
        .client_api
        .try_add_artist(&f.agency, &f.artist, &1_000)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AgreementError::AgencyNotFound);
}

#[test]
fn splits_can_be_renegotiated() {
    let f = setup();
    f.register_with_roster();
    f.client_api.set_artist_split(&f.agency, &f.artist, &3_000);
    assert_eq!(
        f.client_api
            .get_roster_entry(&f.agency, &f.artist)
            .split_bps,
        3_000
    );

    let stranger = Address::generate(&f.env);
    let err = f
        .client_api
        .try_set_artist_split(&f.agency, &stranger, &3_000)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AgreementError::ArtistNotOnRoster);
}

#[test]
fn batch_distribution_splits_and_pays_each_artist() {
    let f = setup();
    f.register_with_roster();

    let total = f.batch(vec![
        &f.env,
        BatchPayment {
            artist: f.artist.clone(),
            gross_usdc: 1_000,
        },
        BatchPayment {
            artist: f.other_artist.clone(),
            gross_usdc: 500,
        },
    ]);
    assert_eq!(total, 1_500);

    // 20% agency cut on 1000, 10% on 500.
    assert_eq!(f.balance(&f.artist), 800);
    assert_eq!(f.balance(&f.other_artist), 450);
    assert_eq!(f.balance(&f.agency), 1_000_000 - 1_250);

    let entry = f.client_api.get_roster_entry(&f.agency, &f.artist);
    assert_eq!(entry.gross_distributed, 1_000);
    assert_eq!(entry.agency_revenue, 200);
    assert_eq!(entry.artist_payouts, 800);
}

#[test]
fn batch_totals_roll_up_into_agency_analytics() {
    let f = setup();
    f.register_with_roster();
    f.batch(vec![
        &f.env,
        BatchPayment {
            artist: f.artist.clone(),
            gross_usdc: 1_000,
        },
    ]);
    f.batch(vec![
        &f.env,
        BatchPayment {
            artist: f.other_artist.clone(),
            gross_usdc: 500,
        },
    ]);

    let analytics = f.client_api.get_agency_analytics(&f.agency);
    assert_eq!(analytics.artist_count, 2);
    assert_eq!(analytics.batches, 2);
    assert_eq!(analytics.gross_distributed, 1_500);
    assert_eq!(analytics.agency_revenue, 250);
    assert_eq!(analytics.artist_payouts, 1_250);
}

#[test]
fn commissions_for_rostered_artists_are_attributed_to_the_agency() {
    let f = setup();
    f.register_with_roster();

    f.client_api.create_agreement(
        &Bytes::from_slice(&f.env, b"comm-1"),
        &f.commissioner,
        &f.artist,
        &String::from_str(&f.env, "Cover art"),
        &5_000,
        &10_000,
    );

    let analytics = f.client_api.get_agency_analytics(&f.agency);
    assert_eq!(analytics.commissions, 1);
    assert_eq!(analytics.commission_budget, 5_000);
    assert_eq!(
        f.client_api
            .get_roster_entry(&f.agency, &f.artist)
            .commissions,
        1
    );
}

#[test]
fn commissions_for_unrepresented_artists_are_not_attributed() {
    let f = setup();
    f.register();
    let independent = Address::generate(&f.env);

    f.client_api.create_agreement(
        &Bytes::from_slice(&f.env, b"comm-1"),
        &f.commissioner,
        &independent,
        &String::from_str(&f.env, "Cover art"),
        &5_000,
        &10_000,
    );

    let analytics = f.client_api.get_agency_analytics(&f.agency);
    assert_eq!(analytics.commissions, 0);
    assert_eq!(analytics.commission_budget, 0);
}

#[test]
fn an_empty_batch_is_rejected() {
    let f = setup();
    f.register_with_roster();
    let err = f
        .client_api
        .try_distribute_batch(&f.agency, &f.token, &Vec::new(&f.env))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AgreementError::EmptyBatch);
}

#[test]
fn a_batch_naming_an_unrostered_artist_is_rejected_whole() {
    let f = setup();
    f.register_with_roster();
    let stranger = Address::generate(&f.env);

    let err = f
        .client_api
        .try_distribute_batch(
            &f.agency,
            &f.token,
            &vec![
                &f.env,
                BatchPayment {
                    artist: f.artist.clone(),
                    gross_usdc: 1_000,
                },
                BatchPayment {
                    artist: stranger.clone(),
                    gross_usdc: 500,
                },
            ],
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AgreementError::ArtistNotOnRoster);

    // Nothing was paid out: the whole batch reverted.
    assert_eq!(f.balance(&f.artist), 0);
    assert_eq!(f.balance(&f.agency), 1_000_000);
}

#[test]
fn non_positive_batch_lines_are_rejected() {
    let f = setup();
    f.register_with_roster();
    let err = f
        .client_api
        .try_distribute_batch(
            &f.agency,
            &f.token,
            &vec![
                &f.env,
                BatchPayment {
                    artist: f.artist.clone(),
                    gross_usdc: 0,
                },
            ],
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AgreementError::InvalidAmount);
}

#[test]
fn a_full_split_leaves_the_artist_nothing_to_transfer() {
    let f = setup();
    f.register();
    f.client_api.add_artist(&f.agency, &f.artist, &10_000);
    f.batch(vec![
        &f.env,
        BatchPayment {
            artist: f.artist.clone(),
            gross_usdc: 1_000,
        },
    ]);
    assert_eq!(f.balance(&f.artist), 0);
    let entry = f.client_api.get_roster_entry(&f.agency, &f.artist);
    assert_eq!(entry.agency_revenue, 1_000);
    assert_eq!(entry.artist_payouts, 0);
}
