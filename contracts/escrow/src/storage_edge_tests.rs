//! Storage edge-case and boundary-value tests (#547).
//!
//! Covers empty values, maximum sizes, invalid ranges, the re-entrancy guard
//! lifecycle (#587), and dispute-TTL configuration defaults/bounds (#586).

extern crate std;

use soroban_sdk::{testutils::Address as _, Address, Bytes, Env};

use crate::errors::EscrowError;
use crate::storage::{
    self, CommissionStatus, DEFAULT_DISPUTE_TTL_LEDGERS, EscrowRecord,
};
use crate::EscrowContract;
use crate::storage::{self, CommissionStatus, DEFAULT_DISPUTE_TTL_LEDGERS, EscrowRecord};
use crate::EscrowContract;

/// SDK 21 requires storage access to run inside a registered contract.
fn with_storage<F, R>(f: F) -> R
where
    F: FnOnce(&Env) -> R,
{
    let env = Env::default();
    let id = env.register_contract(None, EscrowContract);
    env.as_contract(&id, || f(&env))
}

fn make_record(env: &Env, id: &Bytes, amount: i128) -> EscrowRecord {
    EscrowRecord {
        commission_id: id.clone(),
        client: Address::generate(env),
        artist: Address::generate(env),
        amount,
        fee_bps: 500,
        status: CommissionStatus::Locked,
        created_ledger: env.ledger().sequence(),
    }
}

fn bytes(env: &Env, s: &str) -> Bytes {
    Bytes::from_slice(env, s.as_bytes())
}

// ── Empty values (#547) ─────────────────────────────────────────────────────

/// Reading a key that was never written reports absence.
#[test]
fn escrow_exists_false_for_missing_key() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EscrowContract);
    env.as_contract(&contract_id, || {
        assert!(!storage::escrow_exists(&env, &bytes(&env, "ghost")));
    with_storage(|env| {
        assert!(!storage::escrow_exists(env, &bytes(env, "ghost")));
    });
}

/// `get_escrow` on an empty key is an error contract — callers must check
/// existence first; here we document that the has-check is the empty guard.
#[test]
#[should_panic]
fn get_escrow_panics_on_empty_storage() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EscrowContract);
    env.as_contract(&contract_id, || {
        let _ = storage::get_escrow(&env, &bytes(&env, "missing"));
#[should_panic(expected = "called `Option::unwrap()` on a `None` value")]
fn get_escrow_panics_on_empty_storage() {
    with_storage(|env| {
        let _ = storage::get_escrow(env, &bytes(env, "missing"));
    });
}

/// Empty-value guard: zero-length commission ids round-trip like any other.
#[test]
fn empty_commission_id_roundtrip() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EscrowContract);
    env.as_contract(&contract_id, || {
        let id = Bytes::new(&env);
        let record = make_record(&env, &id, 100);

        assert!(!storage::escrow_exists(&env, &id));
        storage::save_escrow(&env, &record);
        assert!(storage::escrow_exists(&env, &id));

        let loaded = storage::get_escrow(&env, &id);
    with_storage(|env| {
        let id = Bytes::new(env);
        let record = make_record(env, &id, 100);

        assert!(!storage::escrow_exists(env, &id));
        storage::save_escrow(env, &record);
        assert!(storage::escrow_exists(env, &id));

        let loaded = storage::get_escrow(env, &id);
        assert_eq!(loaded.amount, 100);
        assert_eq!(loaded.status, CommissionStatus::Locked);
    });
}

// ── Maximum sizes (#547) ────────────────────────────────────────────────────

/// i128::MAX survives a save/load cycle unchanged.
#[test]
fn max_i128_amount_roundtrip() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EscrowContract);
    env.as_contract(&contract_id, || {
        let id = bytes(&env, "max");
        let record = make_record(&env, &id, i128::MAX);
        storage::save_escrow(&env, &record);

        assert_eq!(storage::get_escrow(&env, &id).amount, i128::MAX);
    with_storage(|env| {
        let id = bytes(env, "max");
        let record = make_record(env, &id, i128::MAX);
        storage::save_escrow(env, &record);

        assert_eq!(storage::get_escrow(env, &id).amount, i128::MAX);
    });
}

/// u32::MAX ledger fields are preserved.
#[test]
fn max_u32_fields_roundtrip() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EscrowContract);
    env.as_contract(&contract_id, || {
        let id = bytes(&env, "max-ledger");
        let mut record = make_record(&env, &id, 1);
        record.created_ledger = u32::MAX;
        record.fee_bps = u32::MAX;
        storage::save_escrow(&env, &record);

        let loaded = storage::get_escrow(&env, &id);
    with_storage(|env| {
        let id = bytes(env, "max-ledger");
        let mut record = make_record(env, &id, 1);
        record.created_ledger = u32::MAX;
        record.fee_bps = u32::MAX;
        storage::save_escrow(env, &record);

        let loaded = storage::get_escrow(env, &id);
        assert_eq!(loaded.created_ledger, u32::MAX);
        assert_eq!(loaded.fee_bps, u32::MAX);
    });
}

/// A long commission id (256 bytes) round-trips correctly.
#[test]
fn long_commission_id_roundtrip() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EscrowContract);
    env.as_contract(&contract_id, || {
        let id = Bytes::from_slice(&env, &[0xABu8; 256]);
        let record = make_record(&env, &id, 42);
        storage::save_escrow(&env, &record);
        assert!(storage::escrow_exists(&env, &id));
        assert_eq!(storage::get_escrow(&env, &id).amount, 42);
    with_storage(|env| {
        let id = Bytes::from_slice(env, &[0xABu8; 256]);
        let record = make_record(env, &id, 42);
        storage::save_escrow(env, &record);
        assert!(storage::escrow_exists(env, &id));
        assert_eq!(storage::get_escrow(env, &id).amount, 42);
    });
}

// ── Invalid ranges (#547) ───────────────────────────────────────────────────

/// Negative amounts are storable at the type level but rejected upstream by
/// create_escrow's validation; documents that storage itself is range-neutral.
#[test]
fn negative_amount_roundtrip_documents_upstream_validation() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EscrowContract);
    env.as_contract(&contract_id, || {
        let id = bytes(&env, "negative");
        let record = make_record(&env, &id, -1);
        storage::save_escrow(&env, &record);
        assert_eq!(storage::get_escrow(&env, &id).amount, -1);
    with_storage(|env| {
        let id = bytes(env, "negative");
        let record = make_record(env, &id, -1);
        storage::save_escrow(env, &record);
        assert_eq!(storage::get_escrow(env, &id).amount, -1);
    });
}

/// Overwriting an existing key replaces the previous record entirely.
#[test]
fn save_escrow_overwrites_existing_key() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EscrowContract);
    env.as_contract(&contract_id, || {
        let id = bytes(&env, "dup");
        storage::save_escrow(&env, &make_record(&env, &id, 1));
        let mut updated = make_record(&env, &id, 2);
        updated.status = CommissionStatus::Released;
        storage::save_escrow(&env, &updated);

        let loaded = storage::get_escrow(&env, &id);
    with_storage(|env| {
        let id = bytes(env, "dup");
        storage::save_escrow(env, &make_record(env, &id, 1));
        let mut updated = make_record(env, &id, 2);
        updated.status = CommissionStatus::Released;
        storage::save_escrow(env, &updated);

        let loaded = storage::get_escrow(env, &id);
        assert_eq!(loaded.amount, 2);
        assert_eq!(loaded.status, CommissionStatus::Released);
    });
}

// ── Re-entrancy guard lifecycle (#587) ─────────────────────────────────────

#[test]
fn reentrancy_guard_rejects_nested_call_and_releases_lock() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EscrowContract);
    env.as_contract(&contract_id, || {
        assert!(!storage::is_locked(&env));

        let outer: Result<(), EscrowError> = storage::with_reentrancy_guard(&env, || {
            // Nested call while the lock is held must be rejected.
            let nested: Result<(), EscrowError> =
                storage::with_reentrancy_guard(&env, || Ok(()));
            assert_eq!(nested.unwrap_err(), EscrowError::Reentrant);

            // Lock is visible inside the guarded section…
            assert!(storage::is_locked(&env));
    with_storage(|env| {
        assert!(!storage::is_locked(env));

        let outer: Result<(), EscrowError> = storage::with_reentrancy_guard(env, || {
            // Nested call while the lock is held must be rejected.
            let nested: Result<(), EscrowError> =
                storage::with_reentrancy_guard(env, || Ok(()));
            assert_eq!(nested.unwrap_err(), EscrowError::Reentrant);

            // Lock is visible inside the guarded section…
            assert!(storage::is_locked(env));
            Ok(())
        });

        outer.unwrap();
        // …and released afterwards.
        assert!(!storage::is_locked(&env));
        assert!(!storage::is_locked(env));
    });
}

#[test]
fn reentrancy_guard_releases_on_error_path() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EscrowContract);
    env.as_contract(&contract_id, || {
        let result: Result<(), EscrowError> = storage::with_reentrancy_guard(&env, || {
            Err(EscrowError::InvalidStatus)
        });
        assert!(result.is_err());
        assert!(!storage::is_locked(&env), "lock must clear on error paths");

        // The next call can acquire the lock again.
        let ok: Result<(), EscrowError> = storage::with_reentrancy_guard(&env, || Ok(()));
        ok.unwrap();
        assert!(!storage::is_locked(&env));
    with_storage(|env| {
        let result: Result<(), EscrowError> =
            storage::with_reentrancy_guard(env, || Err(EscrowError::InvalidStatus));
        assert!(result.is_err());
        assert!(!storage::is_locked(env), "lock must clear on error paths");

        // The next call can acquire the lock again.
        let ok: Result<(), EscrowError> = storage::with_reentrancy_guard(env, || Ok(()));
        ok.unwrap();
        assert!(!storage::is_locked(env));
    });
}

#[test]
fn reentrant_error_code_value() {
    assert_eq!(EscrowError::Reentrant as u32, 9);
}

// ── Dispute TTL configuration (#586) ───────────────────────────────────────

#[test]
fn dispute_ttl_defaults_when_unconfigured() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EscrowContract);
    env.as_contract(&contract_id, || {
        assert_eq!(
            storage::get_dispute_ttl_ledgers(&env),
    with_storage(|env| {
        assert_eq!(
            storage::get_dispute_ttl_ledgers(env),
            DEFAULT_DISPUTE_TTL_LEDGERS
        );
        assert_eq!(DEFAULT_DISPUTE_TTL_LEDGERS, 864_000);
    });
}

#[test]
fn dispute_ttl_set_and_get_roundtrip() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EscrowContract);
    env.as_contract(&contract_id, || {
        storage::set_dispute_ttl_ledgers(&env, 1_200_000);
        assert_eq!(storage::get_dispute_ttl_ledgers(&env), 1_200_000);
    with_storage(|env| {
        storage::set_dispute_ttl_ledgers(env, 1_200_000);
        assert_eq!(storage::get_dispute_ttl_ledgers(env), 1_200_000);
    });
}

#[test]
fn dispute_ttl_boundary_values_storable() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EscrowContract);
    env.as_contract(&contract_id, || {
        storage::set_dispute_ttl_ledgers(&env, u32::MAX);
        assert_eq!(storage::get_dispute_ttl_ledgers(&env), u32::MAX);
    with_storage(|env| {
        storage::set_dispute_ttl_ledgers(env, u32::MAX);
        assert_eq!(storage::get_dispute_ttl_ledgers(env), u32::MAX);
    });
}
