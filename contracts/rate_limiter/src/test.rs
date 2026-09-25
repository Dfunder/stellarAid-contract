extern crate std;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

use crate::errors::RateLimitError;
use crate::types::RateLimitKey;
use crate::{RateLimiter, RateLimiterClient};

const WINDOW_LEDGERS: u32 = 100;

struct Fixture<'a> {
    env: Env,
    client: RateLimiterClient<'a>,
    admin: Address,
    artist: Address,
    client_account: Address,
}

fn setup<'a>() -> Fixture<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);

    let contract_id = env.register_contract(None, RateLimiter);
    let client = RateLimiterClient::new(&env, &contract_id);
    client.initialize(&admin);

    Fixture {
        artist: Address::generate(&env),
        client_account: Address::generate(&env),
        env,
        client,
        admin,
    }
}

impl Fixture<'_> {
    fn configure(&self, key: &RateLimitKey, limit: u32) {
        self.client.set_limit(&self.admin, key, &limit, &WINDOW_LEDGERS);
    }

    fn check(&self, key: &RateLimitKey, who: &Address) -> Result<(), RateLimitError> {
        self.client.check_rate_limit(key, who)
    }

    fn advance(&self, ledgers: u32) {
        self.env
            .ledger()
            .with_mut(|l| l.sequence_number += ledgers);
    }
}

// ── Initialization ───────────────────────────────────────────────────────────

#[test]
fn initialize_populates_the_default_limits() {
    let f = setup();
    let key = RateLimitKey::CommissionsPerArtist;
    let config = f.client.get_limit(&key).unwrap();
    assert_eq!(config.limit, 5);
    assert_eq!(config.window_ledgers, 28_800);
}

#[test]
fn double_initialize_is_rejected() {
    let f = setup();
    let err = f.client.try_initialize(&f.admin).err().unwrap().unwrap();
    assert_eq!(err, RateLimitError::AlreadyInitialized);
}

#[test]
fn a_fresh_account_has_no_record() {
    let f = setup();
    let key = RateLimitKey::EscrowsPerUser;
    assert_eq!(f.client.get_count(&key, &f.artist), None);
    assert!(f.client.get_status(&key, &f.artist).is_none());
}

// ── set_limit validation ─────────────────────────────────────────────────────

#[test]
fn zero_limit_is_rejected() {
    let f = setup();
    let err = f
        .client
        .try_set_limit(&f.admin, &RateLimitKey::DisputesPerUser, &0, &WINDOW_LEDGERS)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RateLimitError::InvalidLimit);
}

#[test]
fn zero_window_is_rejected() {
    let f = setup();
    let err = f
        .client
        .try_set_limit(&f.admin, &RateLimitKey::DisputesPerUser, &3, &0)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RateLimitError::InvalidWindow);
}

#[test]
fn set_limit_replaces_the_stored_configuration() {
    let f = setup();
    let key = RateLimitKey::EscrowsPerUser;
    f.configure(&key, 2);
    let config = f.client.get_limit(&key).unwrap();
    assert_eq!(config.limit, 2);
    assert_eq!(config.window_ledgers, WINDOW_LEDGERS);
}

// ── Window behaviour ─────────────────────────────────────────────────────────

#[test]
fn actions_are_allowed_up_to_the_limit() {
    let f = setup();
    let key = RateLimitKey::CommissionsPerArtist;
    f.configure(&key, 3);

    for expected in 1..=3 {
        f.check(&key, &f.artist).unwrap();
        assert_eq!(f.client.get_count(&key, &f.artist), Some(expected));
    }
}

#[test]
fn the_action_past_the_limit_is_rejected() {
    let f = setup();
    let key = RateLimitKey::CommissionsPerArtist;
    f.configure(&key, 2);
    f.check(&key, &f.artist).unwrap();
    f.check(&key, &f.artist).unwrap();

    let err = f.check(&key, &f.artist).err().unwrap();
    assert_eq!(err, RateLimitError::LimitExceeded);
}

#[test]
fn limits_are_tracked_per_account() {
    let f = setup();
    let key = RateLimitKey::EscrowsPerUser;
    f.configure(&key, 1);

    f.check(&key, &f.artist).unwrap();
    assert_eq!(
        f.check(&key, &f.artist).err().unwrap(),
        RateLimitError::LimitExceeded
    );
    // A different account has its own allowance.
    f.check(&key, &f.client_account).unwrap();
    assert_eq!(f.client.get_count(&key, &f.client_account), Some(1));
}

#[test]
fn the_window_resets_once_it_has_elapsed() {
    let f = setup();
    let key = RateLimitKey::CommissionsPerArtist;
    f.configure(&key, 1);

    f.check(&key, &f.artist).unwrap();
    assert_eq!(
        f.check(&key, &f.artist).err().unwrap(),
        RateLimitError::LimitExceeded
    );

    f.advance(WINDOW_LEDGERS + 1);
    f.check(&key, &f.artist).unwrap();
    let status = f.client.get_status(&key, &f.artist).unwrap();
    assert_eq!(status.count, 1);
    assert_eq!(status.first_ledger, f.env.ledger().sequence());
}

#[test]
fn the_window_does_not_reset_early() {
    let f = setup();
    let key = RateLimitKey::CommissionsPerArtist;
    f.configure(&key, 1);

    f.check(&key, &f.artist).unwrap();
    f.advance(WINDOW_LEDGERS);
    assert_eq!(
        f.check(&key, &f.artist).err().unwrap(),
        RateLimitError::LimitExceeded
    );
}

// ── reset_limits ─────────────────────────────────────────────────────────────

#[test]
fn reset_limits_clears_every_key_for_one_account() {
    let f = setup();
    f.configure(&RateLimitKey::CommissionsPerArtist, 1);
    f.configure(&RateLimitKey::DisputesPerUser, 1);
    f.configure(&RateLimitKey::EscrowsPerUser, 1);

    f.check(&RateLimitKey::CommissionsPerArtist, &f.artist).unwrap();
    f.check(&RateLimitKey::DisputesPerUser, &f.artist).unwrap();
    f.check(&RateLimitKey::EscrowsPerUser, &f.artist).unwrap();

    f.client.reset_limits(&f.admin, &f.artist);

    assert!(f
        .client
        .get_status(&RateLimitKey::CommissionsPerArtist, &f.artist)
        .is_none());
    assert!(f
        .client
        .get_status(&RateLimitKey::DisputesPerUser, &f.artist)
        .is_none());
    assert!(f
        .client
        .get_status(&RateLimitKey::EscrowsPerUser, &f.artist)
        .is_none());

    // The cleared account is throttled again from scratch.
    f.check(&RateLimitKey::CommissionsPerArtist, &f.artist).unwrap();
}

#[test]
fn reset_limits_leaves_other_accounts_alone() {
    let f = setup();
    f.configure(&RateLimitKey::EscrowsPerUser, 1);
    f.check(&RateLimitKey::EscrowsPerUser, &f.artist).unwrap();
    f.check(&RateLimitKey::EscrowsPerUser, &f.client_account).unwrap();

    f.client.reset_limits(&f.admin, &f.artist);

    assert_eq!(f.client.get_count(&RateLimitKey::EscrowsPerUser, &f.artist), None);
    assert_eq!(
        f.client.get_count(&RateLimitKey::EscrowsPerUser, &f.client_account),
        Some(1)
    );
}

// ── Error code stability ─────────────────────────────────────────────────────

/// The `#[contracterror]` discriminants are part of the on-chain ABI. Pinning
/// them here means a future renumbering fails the build rather than silently
/// changing the code an SDK decodes.
#[test]
fn error_codes_are_stable() {
    assert_eq!(RateLimitError::NotInitialized as u32, 1);
    assert_eq!(RateLimitError::AlreadyInitialized as u32, 2);
    assert_eq!(RateLimitError::InvalidLimit as u32, 3);
    assert_eq!(RateLimitError::InvalidWindow as u32, 4);
    assert_eq!(RateLimitError::LimitExceeded as u32, 5);
}
