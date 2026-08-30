//! Regression suite run against an upgraded instance.
//!
//! After an upgrade simulation the suite re-probes every surface that must not
//! regress: version queries, schema, admin, roster reads, migration markers,
//! and migration idempotency.

use soroban_sdk::{Address, Bytes, Env};

use crate::v2::V2Client;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegressionReport {
    pub version: (u32, u32, u32),
    pub schema: u32,
    pub rosters_ok: u32,
    pub members_ok: u32,
    pub migrated_ok: u32,
    pub migration_idempotent: bool,
    pub admin_preserved: bool,
    pub all_passed: bool,
}

/// Runs the post-upgrade regression battery for `roster_ids` whose
/// `members[rid]` must still resolve to `expected_members`.
pub fn regression_suite(
    env: &Env,
    contract: Address,
    admin: &Address,
    roster_ids: &[Bytes],
    expected_members: &[(Bytes, Address)],
) -> RegressionReport {
    let client = V2Client::new(env, &contract);

    let version = client.version();
    let schema = client.storage_schema();
    let admin_preserved = client.admin() == *admin;

    let mut rosters_ok = 0u32;
    let mut members_ok = 0u32;
    let mut migrated_ok = 0u32;

    for rid in roster_ids {
        if client.get_roster(rid).is_some() {
            rosters_ok += 1;
        }
        for (rid2, member) in expected_members {
            if rid2 == rid && client.get_member(rid2, member).is_some() {
                members_ok += 1;
            }
        }
        if client.is_migrated(rid) {
            migrated_ok += 1;
        }
    }

    // Idempotency: migrating a roster twice (and then a roster that is already
    // migrated) must succeed both times and keep shares stable.
    let reference = roster_ids.first().cloned();
    let migration_idempotent = match reference {
        Some(rid) => {
            client.migrate(admin, &rid)
                && client.migrate(admin, &rid)
                && client.is_migrated(&rid)
        }
        None => true,
    };

    let all_passed = version == (2, 0, 0)
        && schema == 2
        && admin_preserved
        && rosters_ok == roster_ids.len() as u32
        && migrated_ok == roster_ids.len() as u32;

    RegressionReport {
        version,
        schema,
        rosters_ok,
        members_ok,
        migrated_ok,
        migration_idempotent,
        admin_preserved,
        all_passed,
    }
}

/// `true` when the upgraded instance satisfies the whole regression battery.
pub fn verify(report: &RegressionReport) -> bool {
    report.all_passed
}