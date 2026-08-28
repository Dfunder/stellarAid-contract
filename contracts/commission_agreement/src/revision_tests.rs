//! Commission revision tests (#600).

extern crate std;
use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, String};

use crate::errors::AgreementError;
use crate::revision::RevisionStatus;
use crate::{CommissionAgreementContract, CommissionAgreementContractClient};

const BUDGET: i128 = 1_000;
const DEADLINE: u32 = 10_000;

struct Fixture<'a> {
    env: Env,
    client_api: CommissionAgreementContractClient<'a>,
    client: Address,
    artist: Address,
    commission_id: Bytes,
}

fn setup<'a>() -> Fixture<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, CommissionAgreementContract);
    let client_api = CommissionAgreementContractClient::new(&env, &contract_id);
    Fixture {
        commission_id: Bytes::from_slice(&env, b"comm-rev-001"),
        client: Address::generate(&env),
        artist: Address::generate(&env),
        env,
        client_api,
    }
}

impl Fixture<'_> {
    fn create_and_activate(&self) {
        self.client_api.create_agreement(
            &self.commission_id,
            &self.client,
            &self.artist,
            &String::from_str(&self.env, "Album artwork"),
            &BUDGET,
            &DEADLINE,
        );
        self.client_api.accept_agreement(&self.commission_id);
    }

    fn desc(&self, s: &str) -> String {
        String::from_str(&self.env, s)
    }
}

#[test]
fn artist_can_request_a_revision_with_cost_adjustment() {
    let f = setup();
    f.create_and_activate();

    let idx = f.client_api.request_revision(
        &f.commission_id,
        &f.artist,
        &f.desc("Add a second color pass"),
        &(DEADLINE - 1),
        &150_i128,
    );
    assert_eq!(idx, 0);

    let rev = f.client_api.get_revision(&f.commission_id, &0);
    assert_eq!(rev.status, RevisionStatus::Pending);
    assert_eq!(rev.cost_adjustment, 150);
    assert_eq!(rev.requester, f.artist);
    assert_eq!(f.client_api.get_revision_count(&f.commission_id), 1);
}

#[test]
fn client_accepting_a_revision_adjusts_the_budget() {
    let f = setup();
    f.create_and_activate();

    f.client_api.request_revision(
        &f.commission_id,
        &f.artist,
        &f.desc("Rush delivery"),
        &(DEADLINE - 1),
        &200_i128,
    );
    f.client_api
        .respond_to_revision(&f.commission_id, &f.client, &0, &true, &f.desc("agreed"));

    let agreement = f.client_api.get_agreement(&f.commission_id);
    assert_eq!(agreement.budget_usdc, BUDGET + 200);

    let rev = f.client_api.get_revision(&f.commission_id, &0);
    assert_eq!(rev.status, RevisionStatus::Accepted);
    assert_eq!(rev.response_note, Some(f.desc("agreed")));
}

#[test]
fn rejecting_a_revision_leaves_the_budget_untouched() {
    let f = setup();
    f.create_and_activate();

    f.client_api.request_revision(
        &f.commission_id,
        &f.artist,
        &f.desc("Extra character"),
        &(DEADLINE - 1),
        &500_i128,
    );
    f.client_api.respond_to_revision(
        &f.commission_id,
        &f.client,
        &0,
        &false,
        &f.desc("too expensive"),
    );

    let agreement = f.client_api.get_agreement(&f.commission_id);
    assert_eq!(agreement.budget_usdc, BUDGET);
    assert_eq!(
        f.client_api.get_revision(&f.commission_id, &0).status,
        RevisionStatus::Rejected
    );
}

#[test]
fn client_can_request_changes_and_artist_responds() {
    let f = setup();
    f.create_and_activate();

    f.client_api.request_revision(
        &f.commission_id,
        &f.client,
        &f.desc("Please simplify the background"),
        &(DEADLINE - 1),
        &-100_i128,
    );
    f.client_api
        .respond_to_revision(&f.commission_id, &f.artist, &0, &true, &f.desc("ok"));

    let agreement = f.client_api.get_agreement(&f.commission_id);
    assert_eq!(agreement.budget_usdc, BUDGET - 100);
}

#[test]
fn requester_cannot_respond_to_their_own_revision() {
    let f = setup();
    f.create_and_activate();
    f.client_api.request_revision(
        &f.commission_id,
        &f.artist,
        &f.desc("scope change"),
        &(DEADLINE - 1),
        &0_i128,
    );

    let err = f
        .client_api
        .try_respond_to_revision(&f.commission_id, &f.artist, &0, &true, &f.desc("self-approve"))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AgreementError::RevisionSameParty);
}

#[test]
fn stranger_cannot_request_a_revision() {
    let f = setup();
    f.create_and_activate();
    let stranger = Address::generate(&f.env);

    let err = f
        .client_api
        .try_request_revision(&f.commission_id, &stranger, &f.desc("n/a"), &(DEADLINE - 1), &0_i128)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AgreementError::Unauthorized);
}

#[test]
fn revision_limit_is_enforced() {
    let f = setup();
    f.client_api.create_agreement(
        &f.commission_id,
        &f.client,
        &f.artist,
        &String::from_str(&f.env, "Album artwork"),
        &BUDGET,
        &DEADLINE,
    );
    f.client_api.set_revision_policy(&f.commission_id, &2);
    f.client_api.accept_agreement(&f.commission_id);

    f.client_api
        .request_revision(&f.commission_id, &f.artist, &f.desc("r1"), &(DEADLINE - 1), &0_i128);
    f.client_api
        .request_revision(&f.commission_id, &f.artist, &f.desc("r2"), &(DEADLINE - 1), &0_i128);

    let err = f
        .client_api
        .try_request_revision(&f.commission_id, &f.artist, &f.desc("r3"), &(DEADLINE - 1), &0_i128)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AgreementError::RevisionLimitReached);
}

#[test]
fn default_revision_limit_applies_when_no_policy_is_set() {
    let f = setup();
    f.create_and_activate();
    assert_eq!(
        f.client_api.get_revision_policy(&f.commission_id),
        crate::revision::DEFAULT_MAX_REVISIONS
    );
}

#[test]
fn revision_policy_can_only_be_set_before_acceptance() {
    let f = setup();
    f.client_api.create_agreement(
        &f.commission_id,
        &f.client,
        &f.artist,
        &String::from_str(&f.env, "Album artwork"),
        &BUDGET,
        &DEADLINE,
    );
    f.client_api.accept_agreement(&f.commission_id);

    let err = f
        .client_api
        .try_set_revision_policy(&f.commission_id, &3)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AgreementError::InvalidStatus);
}

#[test]
fn cannot_respond_to_an_already_resolved_revision() {
    let f = setup();
    f.create_and_activate();
    f.client_api.request_revision(
        &f.commission_id,
        &f.artist,
        &f.desc("scope change"),
        &(DEADLINE - 1),
        &0_i128,
    );
    f.client_api
        .respond_to_revision(&f.commission_id, &f.client, &0, &true, &f.desc("ok"));

    let err = f
        .client_api
        .try_respond_to_revision(&f.commission_id, &f.client, &0, &true, &f.desc("again"))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AgreementError::RevisionAlreadyResolved);
}

#[test]
fn accepting_a_revision_that_would_zero_out_the_budget_is_rejected() {
    let f = setup();
    f.create_and_activate();
    f.client_api.request_revision(
        &f.commission_id,
        &f.client,
        &f.desc("huge discount"),
        &(DEADLINE - 1),
        &-BUDGET,
    );

    let err = f
        .client_api
        .try_respond_to_revision(&f.commission_id, &f.artist, &0, &true, &f.desc("ok"))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AgreementError::InvalidAmount);

    // The agreement's budget is unchanged by the failed acceptance.
    assert_eq!(f.client_api.get_agreement(&f.commission_id).budget_usdc, BUDGET);
}

#[test]
fn revision_history_accumulates_across_multiple_requests() {
    let f = setup();
    f.create_and_activate();

    f.client_api
        .request_revision(&f.commission_id, &f.artist, &f.desc("r1"), &(DEADLINE - 1), &10_i128);
    f.client_api
        .respond_to_revision(&f.commission_id, &f.client, &0, &true, &f.desc("ok"));
    f.client_api
        .request_revision(&f.commission_id, &f.client, &f.desc("r2"), &(DEADLINE - 1), &-5_i128);
    f.client_api
        .respond_to_revision(&f.commission_id, &f.artist, &1, &false, &f.desc("no"));

    let history = f.client_api.get_revisions(&f.commission_id);
    assert_eq!(history.len(), 2);
    assert_eq!(history.get(0).unwrap().status, RevisionStatus::Accepted);
    assert_eq!(history.get(1).unwrap().status, RevisionStatus::Rejected);

    let agreement = f.client_api.get_agreement(&f.commission_id);
    assert_eq!(agreement.budget_usdc, BUDGET + 10);
}

#[test]
fn revision_deadline_must_be_in_the_future() {
    let f = setup();
    f.create_and_activate();
    let err = f
        .client_api
        .try_request_revision(&f.commission_id, &f.artist, &f.desc("r1"), &0, &0_i128)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AgreementError::DeadlineInPast);
}

#[test]
fn cannot_request_a_revision_on_a_pending_agreement() {
    let f = setup();
    f.client_api.create_agreement(
        &f.commission_id,
        &f.client,
        &f.artist,
        &String::from_str(&f.env, "Album artwork"),
        &BUDGET,
        &DEADLINE,
    );
    let err = f
        .client_api
        .try_request_revision(&f.commission_id, &f.artist, &f.desc("too early"), &(DEADLINE - 1), &0_i128)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AgreementError::InvalidStatus);
}
