//! Cancellation and pro-rata refund tests (#605).

extern crate std;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    Address, Bytes, Env, IntoVal, String, Symbol,
};

use crate::cancellation::{CancellationPolicy, CancellationReason, Party};
use crate::errors::AgreementError;
use crate::types::AgreementStatus;
use crate::{CommissionAgreementContract, CommissionAgreementContractClient};

const BUDGET: i128 = 1_000;
const DEADLINE: u32 = 10_000;

struct Fixture<'a> {
    env: Env,
    client_api: CommissionAgreementContractClient<'a>,
    contract_id: Address,
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
        commission_id: Bytes::from_slice(&env, b"comm-001"),
        client: Address::generate(&env),
        artist: Address::generate(&env),
        contract_id,
        env,
        client_api,
    }
}

impl Fixture<'_> {
    fn create(&self) {
        self.client_api.create_agreement(
            &self.commission_id,
            &self.client,
            &self.artist,
            &String::from_str(&self.env, "Album artwork"),
            &BUDGET,
            &DEADLINE,
        );
    }

    /// Create, accept, and approve `approved` worth of milestones so the
    /// agreement sits at a known completion percentage. Any remaining budget is
    /// proposed as a second, still-pending milestone, which keeps the agreement
    /// `Active` rather than tipping it into `Completed`.
    fn create_with_progress(&self, approved: i128) {
        self.create();
        self.client_api.accept_agreement(&self.commission_id);
        if approved > 0 {
            let milestone_id = Bytes::from_slice(&self.env, b"ms-1");
            self.client_api.propose_milestone(
                &self.commission_id,
                &milestone_id,
                &String::from_str(&self.env, "Sketches"),
                &approved,
            );
            if approved < BUDGET {
                self.client_api.propose_milestone(
                    &self.commission_id,
                    &Bytes::from_slice(&self.env, b"ms-2"),
                    &String::from_str(&self.env, "Final art"),
                    &(BUDGET - approved),
                );
            }
            self.client_api
                .approve_milestone(&self.commission_id, &milestone_id);
        }
    }
}

#[test]
fn default_policy_applies_when_none_is_set() {
    let f = setup();
    f.create();
    let policy = f.client_api.get_cancellation_policy(&f.commission_id);
    assert_eq!(policy.penalty_bps, 1_000);
    assert_eq!(policy.grace_ledgers, 0);
}

#[test]
fn policy_can_only_be_set_before_acceptance() {
    let f = setup();
    f.create();
    let policy = CancellationPolicy {
        penalty_bps: 2_000,
        grace_ledgers: 50,
    };
    f.client_api
        .set_cancellation_policy(&f.commission_id, &policy);
    assert_eq!(
        f.client_api.get_cancellation_policy(&f.commission_id),
        policy
    );

    f.client_api.accept_agreement(&f.commission_id);
    let err = f
        .client_api
        .try_set_cancellation_policy(&f.commission_id, &policy)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AgreementError::InvalidStatus);
}

#[test]
fn policy_penalty_is_bounded() {
    let f = setup();
    f.create();
    let err = f
        .client_api
        .try_set_cancellation_policy(
            &f.commission_id,
            &CancellationPolicy {
                penalty_bps: 10_001,
                grace_ledgers: 0,
            },
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AgreementError::InvalidPolicy);
}

#[test]
fn refund_is_pro_rata_to_approved_work() {
    let f = setup();
    // 400 of a 1000 budget approved -> 40% complete.
    f.create_with_progress(400);

    let quote = f
        .client_api
        .quote_cancellation(&f.commission_id, &CancellationReason::MutualAgreement);
    assert_eq!(quote.completion_bps, 4_000);
    assert_eq!(quote.earned, 400);
    assert_eq!(quote.penalty, 0);
    assert_eq!(quote.penalised, Party::Neither);
    assert_eq!(quote.artist_amount, 400);
    assert_eq!(quote.client_refund, 600);
}

#[test]
fn client_cancellation_penalty_is_charged_on_the_refund() {
    let f = setup();
    f.create_with_progress(400);

    // Default 10% penalty on the 600 unearned remainder.
    let quote = f
        .client_api
        .quote_cancellation(&f.commission_id, &CancellationReason::ClientRequest);
    assert_eq!(quote.penalised, Party::Client);
    assert_eq!(quote.penalty, 60);
    assert_eq!(quote.artist_amount, 460);
    assert_eq!(quote.client_refund, 540);
    assert_eq!(quote.artist_amount + quote.client_refund, BUDGET);
}

#[test]
fn artist_withdrawal_penalty_is_charged_on_the_earnings() {
    let f = setup();
    f.create_with_progress(400);

    // Default 10% penalty on the 400 the artist earned.
    let quote = f
        .client_api
        .quote_cancellation(&f.commission_id, &CancellationReason::ArtistWithdrawal);
    assert_eq!(quote.penalised, Party::Artist);
    assert_eq!(quote.penalty, 40);
    assert_eq!(quote.artist_amount, 360);
    assert_eq!(quote.client_refund, 640);
}

#[test]
fn cancelling_before_any_work_refunds_the_whole_budget() {
    let f = setup();
    f.create_with_progress(0);
    let quote = f
        .client_api
        .quote_cancellation(&f.commission_id, &CancellationReason::MutualAgreement);
    assert_eq!(quote.completion_bps, 0);
    assert_eq!(quote.artist_amount, 0);
    assert_eq!(quote.client_refund, BUDGET);
}

#[test]
fn grace_window_waives_the_penalty() {
    let f = setup();
    f.create();
    f.client_api.set_cancellation_policy(
        &f.commission_id,
        &CancellationPolicy {
            penalty_bps: 2_000,
            grace_ledgers: 100,
        },
    );
    f.client_api.accept_agreement(&f.commission_id);

    let quote = f
        .client_api
        .quote_cancellation(&f.commission_id, &CancellationReason::ClientRequest);
    assert_eq!(quote.penalty, 0);
    assert_eq!(quote.penalised, Party::Neither);

    f.env.ledger().with_mut(|l| l.sequence_number += 101);
    let quote = f
        .client_api
        .quote_cancellation(&f.commission_id, &CancellationReason::ClientRequest);
    assert_eq!(quote.penalty, 200);
    assert_eq!(quote.penalised, Party::Client);
}

#[test]
fn cancelling_settles_and_records_history() {
    let f = setup();
    f.create_with_progress(400);

    let record = f.client_api.cancel_agreement(
        &f.commission_id,
        &f.client,
        &CancellationReason::ClientRequest,
    );
    assert_eq!(record.completion_bps, 4_000);
    assert_eq!(record.artist_amount, 460);
    assert_eq!(record.client_refund, 540);
    assert_eq!(record.initiator, f.client);

    assert_eq!(
        f.client_api.get_agreement(&f.commission_id).status,
        AgreementStatus::Cancelled
    );
    assert_eq!(f.client_api.get_cancellation(&f.commission_id), record);

    let history = f.client_api.get_cancellation_history();
    assert_eq!(history.len(), 1);
    assert_eq!(history.get(0).unwrap().commission_id, f.commission_id);
}

#[test]
fn cancellation_emits_a_settlement_event() {
    let f = setup();
    f.create_with_progress(400);
    f.client_api.cancel_agreement(
        &f.commission_id,
        &f.artist,
        &CancellationReason::ArtistWithdrawal,
    );

    // The settlement event is the last one published by the call.
    let events = f.env.events().all();
    let (contract, topics, data) = events.last().unwrap();
    assert_eq!(contract, f.contract_id);
    let topic: Symbol = topics.get(0).unwrap().into_val(&f.env);
    assert_eq!(topic, symbol_short!("agr_canc"));

    let (commission_id, initiator, reason, completion_bps, artist_amount, client_refund): (
        Bytes,
        Address,
        CancellationReason,
        u32,
        i128,
        i128,
    ) = data.into_val(&f.env);
    assert_eq!(reason, CancellationReason::ArtistWithdrawal);
    assert_eq!(commission_id, f.commission_id);
    assert_eq!(initiator, f.artist);
    assert_eq!(completion_bps, 4_000);
    assert_eq!(artist_amount, 360);
    assert_eq!(client_refund, 640);
}

#[test]
fn only_the_parties_can_cancel() {
    let f = setup();
    f.create_with_progress(400);
    let outsider = Address::generate(&f.env);
    let err = f
        .client_api
        .try_cancel_agreement(&f.commission_id, &outsider, &CancellationReason::Breach)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AgreementError::Unauthorized);
}

#[test]
fn an_agreement_cannot_be_cancelled_twice() {
    let f = setup();
    f.create_with_progress(400);
    f.client_api.cancel_agreement(
        &f.commission_id,
        &f.client,
        &CancellationReason::MutualAgreement,
    );
    let err = f
        .client_api
        .try_cancel_agreement(
            &f.commission_id,
            &f.client,
            &CancellationReason::MutualAgreement,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AgreementError::AlreadyCancelled);
}

#[test]
fn a_completed_agreement_is_not_cancellable() {
    let f = setup();
    // Approving the full budget completes the agreement.
    f.create_with_progress(BUDGET);
    assert_eq!(
        f.client_api.get_agreement(&f.commission_id).status,
        AgreementStatus::Completed
    );
    let err = f
        .client_api
        .try_cancel_agreement(
            &f.commission_id,
            &f.client,
            &CancellationReason::MutualAgreement,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AgreementError::NotCancellable);
}

#[test]
fn a_pending_agreement_can_be_cancelled_outright() {
    let f = setup();
    f.create();
    let record = f.client_api.cancel_agreement(
        &f.commission_id,
        &f.client,
        &CancellationReason::MutualAgreement,
    );
    assert_eq!(record.completion_bps, 0);
    assert_eq!(record.client_refund, BUDGET);
}

#[test]
fn cancelling_an_unknown_agreement_is_reported() {
    let f = setup();
    let err = f
        .client_api
        .try_cancel_agreement(
            &Bytes::from_slice(&f.env, b"missing"),
            &f.client,
            &CancellationReason::Breach,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AgreementError::NotFound);
}

#[test]
fn settlement_always_sums_to_the_budget() {
    // Odd budgets exercise the flooring in the pro-rata and penalty maths.
    for approved in [1i128, 333, 499, 501, 999] {
        let f = setup();
        f.create_with_progress(approved);
        for reason in [
            CancellationReason::ClientRequest,
            CancellationReason::ArtistWithdrawal,
            CancellationReason::MutualAgreement,
        ] {
            let quote = f.client_api.quote_cancellation(&f.commission_id, &reason);
            assert_eq!(quote.artist_amount + quote.client_refund, BUDGET);
            assert!(quote.artist_amount >= 0);
            assert!(quote.client_refund >= 0);
        }
    }
}
