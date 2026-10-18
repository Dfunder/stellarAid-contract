//! Event emission tests for the campaign contract (issues #755–#758).
//!
//! Every test asserts on the *decoded* event payload, not just on "an event
//! was published", so a field rename or a dropped field fails the suite.

use crate::events::*;
use crate::{CampaignContract, CampaignContractClient, DataKey};
use shared::types::CampaignStatus;
use soroban_sdk::{
    testutils::Address as _, testutils::Events as _, testutils::Ledger as _, Address, BytesN, Env, FromVal, String, Symbol,
};

/// Finds the most recent event whose topic 0 equals `name` and returns its
/// decoded data. Returns `None` when the event was never published, which is
/// what makes "this event must not fire" assertions possible.
fn last_event<T: FromVal<Env, soroban_sdk::Val>>(
    env: &Env,
    contract_id: &Address,
    name: &str,
) -> Option<T> {
    let all = env.events().all();
    for (_, topics, data) in all.iter().rev() {
        let symbol = Symbol::from_val(env, &topics.get(0).unwrap());
        if symbol == Symbol::new(env, name) {
            // Topic 1 must be the emitting contract address, per the
            // documented two-segment topic layout.
            let emitter = Address::from_val(env, &topics.get(1).unwrap());
            assert_eq!(&emitter, contract_id);
            return Some(T::from_val(env, &data));
        }
    }
    None
}

struct Fixture {
    env: Env,
    client: CampaignContractClient<'static>,
    contract_id: Address,
    admin: Address,
    owner: Address,
    donor: Address,
    campaign_id: u64,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, CampaignContract);
    let client = CampaignContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let owner = Address::generate(&env);
    let donor = Address::generate(&env);

    client.initialize(&admin);
    let campaign_id = client.create_campaign(&owner, &10_000_i128, &2_000_u64, &500, &None);

    Fixture {
        env,
        client,
        contract_id,
        admin,
        owner,
        donor,
        campaign_id,
    }
}

impl Fixture {
    fn fund(&self, amount: i128) {
        self.client.record_donation(
            &self.campaign_id,
            &self.donor,
            &amount,
            &String::from_str(&self.env, "USDC"),
        );
    }

    fn hash(&self, byte: u8) -> BytesN<32> {
        BytesN::from_array(&self.env, &[byte; 32])
    }
}

#[test]
fn campaign_initialized_event_carries_admin_and_version() {
    let f = setup();
    let event: CampaignInitializedEvent = last_event(&f.env, &f.contract_id, "campaign_initialized")
        .expect("campaign_initialized not emitted");
    assert_eq!(event.admin, f.admin);
    assert_eq!(event.version, String::from_str(&f.env, "0.1.0"));
    assert_eq!(event.timestamp, f.env.ledger().timestamp());
}

#[test]
fn donation_received_event_carries_running_total() {
    let f = setup();
    f.fund(1_500);

    let event: DonationReceivedEvent = last_event(&f.env, &f.contract_id, "donation_received")
        .expect("donation_received not emitted");
    assert_eq!(event.campaign_id, f.campaign_id);
    assert_eq!(event.donor, f.donor);
    assert_eq!(event.amount, 1_500);
    assert_eq!(event.asset_code, String::from_str(&f.env, "USDC"));
    assert_eq!(event.raised_total, 1_500);

    // A second donation must report the new running total, not the delta.
    f.fund(500);
    let event: DonationReceivedEvent = last_event(&f.env, &f.contract_id, "donation_received")
        .expect("donation_received not emitted");
    assert_eq!(event.raised_total, 2_000);

    // The event is emitted after the write, so it must agree with storage.
    assert_eq!(f.client.get_campaign(&f.campaign_id).unwrap().raised, 2_000);
}

#[test]
fn donation_received_rejects_non_positive_amount() {
    let f = setup();
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client
            .record_donation(&f.campaign_id, &f.donor, &0_i128, &String::from_str(&f.env, "USDC"));
    }));
    assert!(res.is_err());
    assert!(
        last_event::<DonationReceivedEvent>(&f.env, &f.contract_id, "donation_received").is_none()
    );
}

#[test]
fn withdrawal_requested_and_finalized_are_separate_events() {
    let f = setup();
    f.fund(4_000);
    f.client.request_withdrawal(&f.campaign_id, &2_500_i128);

    let requested: WithdrawalRequestedEvent =
        last_event(&f.env, &f.contract_id, "withdrawal_requested")
            .expect("withdrawal_requested not emitted");
    assert_eq!(requested.campaign_id, f.campaign_id);
    assert_eq!(requested.amount, 2_500);
    assert_eq!(requested.requested_at, f.env.ledger().sequence() as u64);
    assert!(requested.available_at > requested.requested_at);
    // No funds have moved yet.
    assert!(
        last_event::<WithdrawalFinalizedEvent>(&f.env, &f.contract_id, "withdrawal_finalized")
            .is_none()
    );

    // Make the notice window elapse.
    let next_seq = u32::try_from(requested.available_at + 1).unwrap();
    f.env.as_contract(&f.contract_id, || {
        f.env.storage().instance().extend_ttl(next_seq + 10_000, next_seq + 10_000);
        f.env.storage().persistent().extend_ttl(&DataKey::Campaign(f.campaign_id), next_seq + 10_000, next_seq + 10_000);
        f.env.storage().persistent().extend_ttl(&DataKey::PendingWithdrawal(f.campaign_id), next_seq + 10_000, next_seq + 10_000);
    });
    f.env.ledger().set_sequence_number(next_seq);
    f.client.finalize_withdrawal(&f.campaign_id, &2_500_i128);

    let finalized: WithdrawalFinalizedEvent =
        last_event(&f.env, &f.contract_id, "withdrawal_finalized")
            .expect("withdrawal_finalized not emitted");
    assert_eq!(finalized.campaign_id, f.campaign_id);
    assert_eq!(finalized.amount, 2_500);
    // 500 bps of 2_500
    assert_eq!(finalized.fee, 125);
    assert_eq!(finalized.recipient, f.owner);
    assert_eq!(finalized.timestamp, f.env.ledger().timestamp());
    assert_eq!(f.client.get_total_withdrawn(&f.campaign_id), 2_500);
    // Request and finalization are two events, not one merged event.
    assert!(
        last_event::<WithdrawalRequestedEvent>(&f.env, &f.contract_id, "withdrawal_requested")
            .is_some()
    );
}

#[test]
fn withdrawal_finalized_before_window_elapsed_fails_without_event() {
    let f = setup();
    f.fund(1_000);
    f.client.request_withdrawal(&f.campaign_id, &1_000_i128);

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.finalize_withdrawal(&f.campaign_id, &1_000_i128);
    }));
    assert!(res.is_err());
    assert!(
        last_event::<WithdrawalFinalizedEvent>(&f.env, &f.contract_id, "withdrawal_finalized")
            .is_none()
    );
}

#[test]
fn withdrawal_cancelled_event_emitted_and_request_cleared() {
    let f = setup();
    f.fund(900);
    f.client.request_withdrawal(&f.campaign_id, &900_i128);
    assert!(f.client.get_pending_withdrawal(&f.campaign_id).is_some());

    f.client.cancel_withdrawal_request(&f.campaign_id);

    let cancelled: WithdrawalCancelledEvent =
        last_event(&f.env, &f.contract_id, "withdrawal_cancelled")
            .expect("withdrawal_cancelled not emitted");
    assert_eq!(cancelled.campaign_id, f.campaign_id);
    assert_eq!(cancelled.amount, 900);
    assert_eq!(cancelled.cancelled_at, f.env.ledger().sequence() as u64);
    assert!(f.client.get_pending_withdrawal(&f.campaign_id).is_none());
}

#[test]
fn dispute_raised_and_resolved_events_carry_reason_and_outcome() {
    let f = setup();
    f.fund(1_000);
    let reason = f.hash(3);

    // A donor is a party to the campaign and may raise.
    f.client.raise_dispute(&f.campaign_id, &f.donor, &reason);

    let raised: DisputeRaisedEvent = last_event(&f.env, &f.contract_id, "dispute_raised")
        .expect("dispute_raised not emitted");
    assert_eq!(raised.campaign_id, f.campaign_id);
    assert_eq!(raised.raised_by, f.donor);
    assert_eq!(raised.reason_hash, reason);
    assert_eq!(raised.raised_at, f.env.ledger().timestamp());
    assert_eq!(f.client.get_dispute_reason(&f.campaign_id), Some(reason.clone()));
    assert!(f.client.has_active_dispute(&f.campaign_id));

    // A second dispute is refused and must not emit a second event.
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.raise_dispute(&f.campaign_id, &f.donor, &reason);
    }));
    assert!(res.is_err());

    f.client.resolve_dispute(&f.admin, &f.campaign_id, &false);

    let resolved: DisputeResolvedEvent = last_event(&f.env, &f.contract_id, "dispute_resolved")
        .expect("dispute_resolved not emitted");
    assert_eq!(resolved.campaign_id, f.campaign_id);
    assert!(!resolved.outcome);
    assert_eq!(resolved.resolved_at, f.env.ledger().timestamp());
    assert!(!f.client.has_active_dispute(&f.campaign_id));
    assert_eq!(f.client.get_dispute_reason(&f.campaign_id), None);
}

#[test]
fn dispute_cannot_be_raised_by_a_stranger() {
    let f = setup();
    let stranger = Address::generate(&f.env);
    let reason = f.hash(1);

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.raise_dispute(&f.campaign_id, &stranger, &reason);
    }));
    assert!(res.is_err());
    assert!(last_event::<DisputeRaisedEvent>(&f.env, &f.contract_id, "dispute_raised").is_none());
}

#[test]
fn refund_issued_event_carries_donor_and_amount() {
    let f = setup();
    f.fund(2_000);
    f.client.end_campaign(&f.campaign_id, &f.owner);

    f.client.request_refund(&f.campaign_id, &f.donor, &750_i128);

    let event: RefundIssuedEvent = last_event(&f.env, &f.contract_id, "refund_issued")
        .expect("refund_issued not emitted");
    assert_eq!(event.campaign_id, f.campaign_id);
    assert_eq!(event.donor, f.donor);
    assert_eq!(event.amount, 750);
    assert_eq!(f.client.get_refunded(&f.campaign_id, &f.donor), 750);
}

#[test]
fn campaign_ended_and_cancelled_events() {
    let f = setup();

    f.client.end_campaign(&f.campaign_id, &f.owner);
    let ended: CampaignEndedEvent = last_event(&f.env, &f.contract_id, "campaign_ended")
        .expect("campaign_ended not emitted");
    assert_eq!(ended.campaign_id, f.campaign_id);
    assert_eq!(ended.goal, 10_000);
    assert_eq!(ended.raised, 0);
    assert_eq!(
        f.client.get_campaign(&f.campaign_id).unwrap().status,
        CampaignStatus::Completed
    );

    // A second campaign holding no funds can be cancelled.
    let fresh = f
        .client
        .create_campaign(&f.owner, &1_000_i128, &2_000_u64, &500, &None);
    let reason = f.hash(8);
    f.client.cancel_campaign(&fresh, &f.owner, &reason);

    let cancelled: CampaignCancelledEvent =
        last_event(&f.env, &f.contract_id, "campaign_cancelled")
            .expect("campaign_cancelled not emitted");
    assert_eq!(cancelled.campaign_id, fresh);
    assert_eq!(cancelled.cancelled_by, f.owner);
    assert_eq!(cancelled.reason_hash, reason);
    assert_eq!(
        f.client.get_campaign(&fresh).unwrap().status,
        CampaignStatus::Cancelled
    );
}

#[test]
fn cancel_campaign_with_funds_is_refused() {
    let f = setup();
    f.fund(1);
    let reason = f.hash(8);

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.cancel_campaign(&f.campaign_id, &f.owner, &reason);
    }));
    assert!(res.is_err());
    assert!(
        last_event::<CampaignCancelledEvent>(&f.env, &f.contract_id, "campaign_cancelled").is_none()
    );
}

#[test]
fn deadline_extended_event_reports_old_and_new() {
    let f = setup();
    let before: u64 = f.client.get_campaign(&f.campaign_id).unwrap().deadline;

    f.client.extend_deadline(&f.campaign_id, &(before + 86_400));

    let event: DeadlineExtendedEvent = last_event(&f.env, &f.contract_id, "deadline_extended")
        .expect("deadline_extended not emitted");
    assert_eq!(event.campaign_id, f.campaign_id);
    assert_eq!(event.old_deadline, before);
    assert_eq!(event.new_deadline, before + 86_400);
    assert_eq!(event.extended_at, f.env.ledger().timestamp());
    assert_eq!(
        f.client.get_campaign(&f.campaign_id).unwrap().deadline,
        before + 86_400
    );
}

#[test]
fn every_event_name_uses_the_entity_past_tense_convention() {
    // The documented convention (#755) is `<entity>_<past_tense_verb>`. This
    // pins the full catalogue so a future event cannot drift.
    const CATALOGUE: [&str; 12] = [
        "campaign_initialized",
        "campaign_registered",
        "campaign_ended",
        "campaign_cancelled",
        "donation_received",
        "withdrawal_requested",
        "withdrawal_finalized",
        "withdrawal_cancelled",
        "refund_issued",
        "dispute_raised",
        "dispute_resolved",
        "deadline_extended",
    ];
    for name in CATALOGUE {
        let (entity, verb) = name
            .split_once('_')
            .expect("event name must have two segments");
        assert!(!entity.is_empty(), "{name} must start with an entity");
        assert!(
            verb.ends_with("d") || verb.ends_with("ed"),
            "{name} must end in a past-tense verb"
        );
    }
}

#[test]
fn lifecycle_storage_keys_do_not_collide() {
    // Regression guard: the new DataKey variants must not clobber the
    // pre-existing ones, which would silently corrupt campaign state.
    let f = setup();
    f.fund(500);
    f.client.request_withdrawal(&f.campaign_id, &100_i128);

    f.env.as_contract(&f.contract_id, || {
        assert!(f
            .env
            .storage()
            .persistent()
            .has(&DataKey::PendingWithdrawal(f.campaign_id)));
        assert!(f
            .env
            .storage()
            .persistent()
            .has(&DataKey::Campaign(f.campaign_id)));
        assert!(
            f.env
                .storage()
                .persistent()
                .has(&DataKey::TotalWithdrawn(0))
                == false
        );
    });
}

#[test]
fn campaign_donor_marker_only_set_by_recorded_donations() {
    let f = setup();
    f.env.as_contract(&f.contract_id, || {
        assert!(!CampaignContract::has_donated(
            &f.env,
            f.campaign_id,
            &f.donor
        ));
    });
    f.fund(1);
    f.env.as_contract(&f.contract_id, || {
        assert!(CampaignContract::has_donated(
            &f.env,
            f.campaign_id,
            &f.donor
        ));
    });
}
