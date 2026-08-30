//! Upgrade test scenarios (closes #689).
//!
//! Each scenario mirrors an entry in [docs/UPGRADE_TESTING.md](../../docs/UPGRADE_TESTING.md).

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{vec, Address, Bytes};

use crate::regression::{regression_suite, verify};
use crate::simulator::{UpgradeSimulator, migrate_RosterToShares, migrate_SchemaCompatible};
use crate::v1::V1Client;
use crate::v2::V2Client;

/// Seeds a v1 contract with `count` rosters of 3 members each; returns the ids.
fn seed_v1(
    sim: &UpgradeSimulator,
    id: &Address,
    admin: &Address,
    count: u32,
) -> (Vec<Bytes>, Vec<(Bytes, Address)>) {
    let client = V1Client::new(&sim.env, id);
    let mut ids = Vec::new();
    let mut expected = Vec::new();
    for i in 0..count {
        let rid = Bytes::from_slice(&sim.env, format!("roster-{}", i).as_bytes());
        let members = vec![
            &sim.env,
            Address::generate(&sim.env),
            Address::generate(&sim.env),
            Address::generate(&sim.env),
        ];
        client.add_member(&rid, &members, admin);
        ids.push(rid.clone());
        for m in members.iter() {
            expected.push((rid.clone(), m));
        }
    }
    (ids, expected)
}

/// A schema-compatible WASM swap must leave all v1 records readable and bump
/// the storage-schema version.
#[test]
fn schema_compatible_upgrade_preserves_state() {
    let sim = UpgradeSimulator::new();
    sim.env.mock_all_auths();
    let admin = Address::generate(&sim.env);
    let v1 = sim.deploy_v1();
    V1Client::new(&sim.env, &v1).initialize(&admin);
    let (ids, _expected) = seed_v1(&sim, &v1, &admin, 3);

    let v2 = sim.upgrade_in_place(v1, &ids, migrate_SchemaCompatible);
    let client = V2Client::new(&sim.env, &v2);

    assert_eq!(client.version(), (2, 0, 0));
    assert_eq!(client.storage_schema(), 2);
    for rid in &ids {
        let roster = client.get_roster(rid).expect("roster preserved");
        assert_eq!(roster.len(), 3);
    }
}

/// After a migration entry point runs, its newly derived keys are present and
/// the whole operation is idempotent.
#[test]
fn state_migration_populates_new_keys() {
    let sim = UpgradeSimulator::new();
    sim.env.mock_all_auths();
    let admin = Address::generate(&sim.env);
    let v1 = sim.deploy_v1();
    V1Client::new(&sim.env, &v1).initialize(&admin);
    let (ids, expected) = seed_v1(&sim, &v1, &admin, 2);
    let rid = ids.first().expect("one roster").clone();
    let member = expected
        .iter()
        .find(|(r, _)| *r == rid)
        .map(|(_, m)| m.clone())
        .expect("a member");

    // Swap WASM (schema-compatible), then run the on-chain migration entry point.
    let v2 = sim.upgrade_in_place(v1, &ids, migrate_SchemaCompatible);
    let client = V2Client::new(&sim.env, &v2);

    assert_eq!(client.migrate(&admin, &rid), true);
    assert!(client.is_migrated(&rid));
    // 3 members → equal split 10000/3 = 3333 bps each.
    assert_eq!(client.get_member(&rid, &member), Some(3_333));

    // Idempotent: re-running is a successful no-op.
    assert_eq!(client.migrate(&admin, &rid), true);
    assert_eq!(client.get_member(&rid, &member), Some(3_333));
}

/// v1-written records must deserialize correctly under the v2 code without any
/// migration running first (ABI/backward compatibility).
#[test]
fn backward_compat_keeps_old_records_readable() {
    let sim = UpgradeSimulator::new();
    sim.env.mock_all_auths();
    let admin = Address::generate(&sim.env);
    let v1 = sim.deploy_v1();
    V1Client::new(&sim.env, &v1).initialize(&admin);
    let (ids, _) = seed_v1(&sim, &v1, &admin, 1);
    let rid = ids[0].clone();

    let v2 = sim.upgrade_in_place(v1, &ids, migrate_SchemaCompatible);
    let client = V2Client::new(&sim.env, &v2);
    assert!(
        client.get_roster(&rid).is_some(),
        "legacy record layout must stay readable by v2"
    );
}

/// The framework's schema-changing transform (equal-split shares) is a second
/// demonstration of "simulate the migration, then verify".
#[test]
fn framework_transform_migrates_restored_state() {
    let sim = UpgradeSimulator::new();
    sim.env.mock_all_auths();
    let admin = Address::generate(&sim.env);
    let v1 = sim.deploy_v1();
    V1Client::new(&sim.env, &v1).initialize(&admin);
    let (ids, expected) = seed_v1(&sim, &v1, &admin, 1);
    let rid = ids[0].clone();

    let v2 = sim.upgrade_in_place(v1, &[rid.clone()], migrate_RosterToShares);
    let client = V2Client::new(&sim.env, &v2);
    assert!(client.is_migrated(&rid));
    for (r, m) in expected.iter().filter(|(r, _)| *r == rid) {
        assert!(client.get_member(r, m).is_some());
    }
}

/// The regression suite must pass against the upgraded instance.
#[test]
fn regression_suite_runs_after_upgrade() {
    let sim = UpgradeSimulator::new();
    sim.env.mock_all_auths();
    let admin = Address::generate(&sim.env);
    let v1 = sim.deploy_v1();
    V1Client::new(&sim.env, &v1).initialize(&admin);
    let (ids, expected) = seed_v1(&sim, &v1, &admin, 2);

    let v2 = sim.upgrade_in_place(v1, &ids, migrate_SchemaCompatible);
    let client = V2Client::new(&sim.env, &v2);
    for rid in &ids {
        assert_eq!(client.migrate(&admin, rid), true);
    }

    let report = regression_suite(&sim.env, v2, &admin, &ids, &expected);
    assert!(report.all_passed, "regression battery must pass");
    assert!(verify(&report));
}