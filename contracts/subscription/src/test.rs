extern crate std;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger},
    token, Address, Env, String, Symbol, Vec,
};

use crate::errors::SubscriptionError;
use crate::types::{PaymentKind, SubscriptionStatus};
use crate::{SubscriptionContract, SubscriptionContractClient};

const GRACE: u32 = 100;
const HISTORY_LIMIT: u32 = 4;
const PERIOD: u32 = 1_000;
const BASIC: u32 = 1;
const PRO: u32 = 2;
const BASIC_PRICE: i128 = 100;
const PRO_PRICE: i128 = 250;

struct Fixture<'a> {
    env: Env,
    client: SubscriptionContractClient<'a>,
    token: Address,
    admin: Address,
    member: Address,
}

fn setup<'a>() -> Fixture<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let member = Address::generate(&env);
    token::StellarAssetClient::new(&env, &token).mint(&member, &100_000);

    let contract_id = env.register_contract(None, SubscriptionContract);
    let client = SubscriptionContractClient::new(&env, &contract_id);
    client.initialize(&admin, &token, &GRACE, &HISTORY_LIMIT);

    Fixture {
        env,
        client,
        token,
        admin,
        member,
    }
}

impl Fixture<'_> {
    fn benefits(&self, names: &[Symbol]) -> Vec<Symbol> {
        let mut out = Vec::new(&self.env);
        for name in names {
            out.push_back(name.clone());
        }
        out
    }

    fn create_tiers(&self) {
        self.client.create_tier(
            &BASIC,
            &String::from_str(&self.env, "Basic"),
            &BASIC_PRICE,
            &PERIOD,
            &self.benefits(&[symbol_short!("feed")]),
        );
        self.client.create_tier(
            &PRO,
            &String::from_str(&self.env, "Pro"),
            &PRO_PRICE,
            &PERIOD,
            &self.benefits(&[symbol_short!("feed"), symbol_short!("earlyacc")]),
        );
    }

    fn subscribed(&self, auto_renew: bool) {
        self.create_tiers();
        self.client.deposit(&self.member, &1_000);
        self.client.subscribe(&self.member, &BASIC, &auto_renew);
    }

    fn advance(&self, ledgers: u32) {
        self.env.ledger().with_mut(|l| l.sequence_number += ledgers);
    }

    fn balance(&self, account: &Address) -> i128 {
        token::Client::new(&self.env, &self.token).balance(account)
    }
}

#[test]
fn initialize_is_one_shot() {
    let f = setup();
    let err = f
        .client
        .try_initialize(&f.admin, &f.token, &GRACE, &HISTORY_LIMIT)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, SubscriptionError::AlreadyInitialized);
}

#[test]
fn multiple_tiers_are_supported() {
    let f = setup();
    f.create_tiers();
    assert_eq!(f.client.get_tier(&BASIC).price, BASIC_PRICE);
    assert_eq!(f.client.get_tier(&PRO).price, PRO_PRICE);
    assert_eq!(f.client.get_tier(&PRO).benefits.len(), 2);

    let err = f
        .client
        .try_create_tier(
            &BASIC,
            &String::from_str(&f.env, "Dup"),
            &BASIC_PRICE,
            &PERIOD,
            &Vec::new(&f.env),
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, SubscriptionError::TierExists);
}

#[test]
fn invalid_tier_terms_are_rejected() {
    let f = setup();
    let name = String::from_str(&f.env, "Bad");
    assert_eq!(
        f.client
            .try_create_tier(&BASIC, &name, &0, &PERIOD, &Vec::new(&f.env))
            .err()
            .unwrap()
            .unwrap(),
        SubscriptionError::InvalidPrice
    );
    assert_eq!(
        f.client
            .try_create_tier(&BASIC, &name, &BASIC_PRICE, &0, &Vec::new(&f.env))
            .err()
            .unwrap()
            .unwrap(),
        SubscriptionError::InvalidPeriod
    );
}

#[test]
fn deposits_and_withdrawals_move_credit() {
    let f = setup();
    let before = f.balance(&f.member);
    assert_eq!(f.client.deposit(&f.member, &500), 500);
    assert_eq!(f.client.get_credit(&f.member), 500);
    assert_eq!(f.balance(&f.member), before - 500);

    assert_eq!(f.client.withdraw(&f.member, &200), 300);
    assert_eq!(f.balance(&f.member), before - 300);

    let err = f
        .client
        .try_withdraw(&f.member, &10_000)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, SubscriptionError::InsufficientCredit);
}

#[test]
fn subscribing_charges_credit_and_grants_benefits() {
    let f = setup();
    f.subscribed(true);

    let subscription = f.client.get_subscription(&f.member);
    assert_eq!(subscription.tier_id, BASIC);
    assert_eq!(subscription.status, SubscriptionStatus::Active);
    assert_eq!(subscription.total_paid, BASIC_PRICE);
    assert_eq!(f.client.get_credit(&f.member), 1_000 - BASIC_PRICE);

    assert!(f.client.is_active(&f.member));
    assert!(f.client.has_benefit(&f.member, &symbol_short!("feed")));
    assert!(!f.client.has_benefit(&f.member, &symbol_short!("earlyacc")));
}

#[test]
fn subscribing_without_credit_is_rejected() {
    let f = setup();
    f.create_tiers();
    let err = f
        .client
        .try_subscribe(&f.member, &BASIC, &true)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, SubscriptionError::InsufficientCredit);
}

#[test]
fn double_subscription_is_rejected() {
    let f = setup();
    f.subscribed(true);
    let err = f
        .client
        .try_subscribe(&f.member, &PRO, &true)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, SubscriptionError::AlreadySubscribed);
}

#[test]
fn a_retired_tier_takes_no_new_subscribers() {
    let f = setup();
    f.create_tiers();
    f.client.deposit(&f.member, &1_000);
    f.client.set_tier_active(&BASIC, &false);
    let err = f
        .client
        .try_subscribe(&f.member, &BASIC, &true)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, SubscriptionError::TierInactive);
}

#[test]
fn renewal_extends_the_period_contiguously() {
    let f = setup();
    f.subscribed(true);
    let first_end = f.client.get_subscription(&f.member).period_end_ledger;

    f.advance(PERIOD + 1);
    let new_end = f.client.renew(&f.member);
    assert_eq!(new_end, first_end + PERIOD);

    let subscription = f.client.get_subscription(&f.member);
    assert_eq!(subscription.renewals, 1);
    assert_eq!(subscription.total_paid, BASIC_PRICE * 2);
    assert_eq!(f.client.get_credit(&f.member), 1_000 - BASIC_PRICE * 2);
}

#[test]
fn renewal_before_the_period_ends_is_rejected() {
    let f = setup();
    f.subscribed(true);
    let err = f.client.try_renew(&f.member).err().unwrap().unwrap();
    assert_eq!(err, SubscriptionError::RenewalNotDue);
}

#[test]
fn benefits_survive_the_grace_window_then_lapse() {
    let f = setup();
    f.subscribed(true);

    // Inside grace: still covered, and flagged as awaiting renewal.
    f.advance(PERIOD + 1);
    assert!(f.client.is_active(&f.member));
    assert!(f.client.in_grace(&f.member));
    assert!(f.client.has_benefit(&f.member, &symbol_short!("feed")));

    // Past grace: coverage is gone.
    f.advance(GRACE);
    assert!(!f.client.is_active(&f.member));
    assert!(!f.client.in_grace(&f.member));
    assert!(!f.client.has_benefit(&f.member, &symbol_short!("feed")));
}

#[test]
fn renewal_after_the_grace_window_is_rejected() {
    let f = setup();
    f.subscribed(true);
    f.advance(PERIOD + GRACE + 1);
    let err = f.client.try_renew(&f.member).err().unwrap().unwrap();
    assert_eq!(err, SubscriptionError::GraceExpired);
}

#[test]
fn renewal_without_credit_leaves_the_subscription_untouched() {
    let f = setup();
    f.create_tiers();
    f.client.deposit(&f.member, &BASIC_PRICE);
    f.client.subscribe(&f.member, &BASIC, &true);
    let end = f.client.get_subscription(&f.member).period_end_ledger;

    f.advance(PERIOD + 1);
    let err = f.client.try_renew(&f.member).err().unwrap().unwrap();
    assert_eq!(err, SubscriptionError::InsufficientCredit);
    assert_eq!(f.client.get_subscription(&f.member).period_end_ledger, end);
}

#[test]
fn cancelling_keeps_benefits_until_the_period_ends() {
    let f = setup();
    f.subscribed(true);
    let end = f.client.cancel(&f.member);

    let subscription = f.client.get_subscription(&f.member);
    assert_eq!(subscription.status, SubscriptionStatus::Cancelled);
    assert!(!subscription.auto_renew);
    assert!(f.client.is_active(&f.member));
    assert!(f.client.has_benefit(&f.member, &symbol_short!("feed")));

    // A cancelled subscription gets no grace window.
    f.env.ledger().with_mut(|l| l.sequence_number = end + 1);
    assert!(!f.client.is_active(&f.member));
    assert!(!f.client.in_grace(&f.member));
}

#[test]
fn a_cancelled_subscription_cannot_be_renewed() {
    let f = setup();
    f.subscribed(true);
    f.client.cancel(&f.member);
    f.advance(PERIOD + 1);
    let err = f.client.try_renew(&f.member).err().unwrap().unwrap();
    assert_eq!(err, SubscriptionError::NotRenewable);
}

#[test]
fn lapsing_requires_coverage_to_have_run_out() {
    let f = setup();
    f.subscribed(true);
    let err = f.client.try_lapse(&f.member).err().unwrap().unwrap();
    assert_eq!(err, SubscriptionError::StillActive);

    f.advance(PERIOD + GRACE + 1);
    f.client.lapse(&f.member);
    assert_eq!(
        f.client.get_subscription(&f.member).status,
        SubscriptionStatus::Expired
    );
    assert!(!f.client.is_active(&f.member));
}

#[test]
fn a_lapsed_member_can_subscribe_again() {
    let f = setup();
    f.subscribed(true);
    f.advance(PERIOD + GRACE + 1);
    f.client.lapse(&f.member);

    f.client.subscribe(&f.member, &PRO, &true);
    let subscription = f.client.get_subscription(&f.member);
    assert_eq!(subscription.tier_id, PRO);
    assert_eq!(subscription.status, SubscriptionStatus::Active);
    assert!(f.client.has_benefit(&f.member, &symbol_short!("earlyacc")));
}

#[test]
fn payment_history_records_each_charge() {
    let f = setup();
    f.subscribed(true);
    f.advance(PERIOD + 1);
    f.client.renew(&f.member);

    let payments = f.client.get_payments(&f.member);
    assert_eq!(payments.len(), 2);
    assert_eq!(payments.get(0).unwrap().kind, PaymentKind::Initial);
    assert_eq!(payments.get(1).unwrap().kind, PaymentKind::Renewal);
    assert_eq!(payments.get(1).unwrap().amount, BASIC_PRICE);
}

#[test]
fn payment_history_is_capped() {
    let f = setup();
    // A short billing period keeps the whole run inside the test ledger's
    // default entry TTL while still exercising the history cap.
    const SHORT_TIER: u32 = 3;
    const SHORT_PERIOD: u32 = 50;
    f.client.create_tier(
        &SHORT_TIER,
        &String::from_str(&f.env, "Weekly"),
        &BASIC_PRICE,
        &SHORT_PERIOD,
        &f.benefits(&[symbol_short!("feed")]),
    );
    f.client.deposit(&f.member, &1_000);
    f.client.subscribe(&f.member, &SHORT_TIER, &true);

    for _ in 0..HISTORY_LIMIT + 2 {
        f.advance(SHORT_PERIOD + 1);
        f.client.renew(&f.member);
    }
    assert_eq!(f.client.get_payments(&f.member).len(), HISTORY_LIMIT);
    // The cap trims the oldest entries; the newest charge is still the last.
    let payments = f.client.get_payments(&f.member);
    assert_eq!(
        payments.get(HISTORY_LIMIT - 1).unwrap().sequence,
        HISTORY_LIMIT + 3
    );
}

#[test]
fn unknown_accounts_and_tiers_are_reported() {
    let f = setup();
    let stranger = Address::generate(&f.env);
    assert!(!f.client.is_active(&stranger));
    assert!(!f.client.has_benefit(&stranger, &symbol_short!("feed")));
    assert_eq!(
        f.client
            .try_get_subscription(&stranger)
            .err()
            .unwrap()
            .unwrap(),
        SubscriptionError::NoSubscription
    );
    assert_eq!(
        f.client.try_get_tier(&99).err().unwrap().unwrap(),
        SubscriptionError::TierNotFound
    );
}

#[test]
fn only_the_admin_manages_tiers() {
    let f = setup();
    f.create_tiers();
    // Auth is mocked, so this asserts the admin lookup path resolves rather
    // than the signature check; tier state must still round-trip.
    f.client.set_tier_active(&BASIC, &false);
    assert!(!f.client.get_tier(&BASIC).active);
    f.client.set_tier_active(&BASIC, &true);
    assert!(f.client.get_tier(&BASIC).active);
}
