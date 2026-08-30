//! Assertion helpers for integration scenarios (#663).
//!
//! These keep scenario steps free of hand-rolled storage/event unwrapping so
//! test intent stays readable.

use escrow::storage::CommissionStatus;
use escrow::EscrowContractClient;
use platform_config::PlatformConfigContractClient;
use soroban_sdk::testutils::Events as _;
use soroban_sdk::TryFromVal;
use soroban_sdk::{Bytes, Symbol};

use crate::Environment;

/// Assert an escrow record holds the expected status.
pub fn assert_escrow_status(
    _env: &Environment,
    escrow: &EscrowContractClient,
    commission_id: &Bytes,
    expected: CommissionStatus,
) {
    let record = escrow.get_escrow(commission_id);
    assert_eq!(record.status, expected, "unexpected escrow status");
}

/// Assert the USDC balance of an account.
pub fn assert_balance(env: &Environment, account: &soroban_sdk::Address, expected: i128) {
    assert_eq!(env.balance(account), expected, "unexpected USDC balance");
}

/// Assert the config contract resolves `name` through the active environment
/// to exactly `expected` (dependency-injection check, #662).
pub fn assert_address_resolves(
    config: &PlatformConfigContractClient,
    name: &Symbol,
    expected: &soroban_sdk::Address,
) {
    let resolved = config.resolve_for_environment(name);
    assert_eq!(resolved, *expected, "registry resolution mismatch");
}

/// Assert an event with exactly the given topic symbols was emitted by
/// `contract` at some point during the scenario.
pub fn assert_event_emitted(
    env: &Environment,
    contract: &soroban_sdk::Address,
    topics: &[Symbol],
) {
    let events = env.env.events().all();
    let found = events.iter().any(|(c, t, _d)| {
        c == *contract
            && t.len() as usize == topics.len()
            && t.iter().zip(topics.iter()).all(|(v, s)| {
                Symbol::try_from_val(&env.env, &v)
                    .map(|decoded| &decoded == s)
                    .unwrap_or(false)
            })
    });
    assert!(found, "expected the event to have been emitted");
}

/// Assert at least one correlation event whose first two topics match
/// `domain`/`action` was emitted (schema from #661).
pub fn assert_correlation_event_present(env: &Environment, domain: Symbol, action: Symbol) {
    let events = env.env.events().all();
    let found = events.iter().any(|(_c, t, _d)| {
        t.len() == 3
            && Symbol::try_from_val(&env.env, &t.get(0).unwrap())
                .map(|x| x == domain)
                .unwrap_or(false)
            && Symbol::try_from_val(&env.env, &t.get(1).unwrap())
                .map(|x| x == action)
                .unwrap_or(false)
            && Symbol::try_from_val(&env.env, &t.get(2).unwrap())
                .map(|x| x == Symbol::new(&env.env, "corr"))
                .unwrap_or(false)
    });
    assert!(found, "expected a correlation event");
}