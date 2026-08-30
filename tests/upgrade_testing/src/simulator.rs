//! Upgrade simulator: snapshot, migrate, restore, verify.
//!
//! Models the Soroban upgrade as an in-place switch: the simulator captures the
//! live contract's stored state ("snapshot"), applies the upgrade's migration
//! logic to that snapshot, then restores the migrated snapshot into the v2
//! contract the way a migrated, upgraded contract would hold it.

use soroban_sdk::{Address, Bytes, Env};

use crate::keys::DataKey;
use crate::v1::V1;
use crate::v2::V2;

/// A typed snapshot of a contract's known storage.
#[derive(Clone, Default)]
pub struct StorageSnapshot {
    pub admin: Option<Address>,
    /// Storage schema version as read from the instance key.
    pub schema: u32,
    /// `Roster(id)` persistent records.
    pub rosters: Vec<(Bytes, Vec<Address>)>,
    /// `Relay(id)` migration-completed markers.
    pub relays: Vec<(Bytes, bool)>,
    /// `Member(id, address)` share records (v2 only).
    pub members: Vec<(Bytes, Address, u32)>,
}

/// The in-memory upgrade environment.
pub struct UpgradeSimulator {
    pub env: Env,
}

impl Default for UpgradeSimulator {
    fn default() -> Self {
        Self::new()
    }
}

impl UpgradeSimulator {
    pub fn new() -> Self {
        UpgradeSimulator {
            env: Env::default(),
        }
    }

    /// Deploy the current (v1) contract.
    pub fn deploy_v1(&self) -> Address {
        self.env.register_contract(None, V1)
    }

    /// Deploy the upgraded (v2) contract.
    pub fn deploy_v2(&self) -> Address {
        self.env.register_contract(None, V2)
    }

    /// Capture instance + persistent state for the given roster ids.
    pub fn snapshot(&self, id: &Address, roster_ids: &[Bytes]) -> StorageSnapshot {
        self.env.as_contract(id, || {
            let mut snap = StorageSnapshot {
                admin: self.env.storage().instance().get(&DataKey::Admin),
                schema: self
                    .env
                    .storage()
                    .instance()
                    .get::<DataKey, u32>(&DataKey::Schema)
                    .unwrap_or(0),
                rosters: Vec::new(),
                relays: Vec::new(),
                members: Vec::new(),
            };
            for rid in roster_ids {
                if let Some(r) = self
                    .env
                    .storage()
                    .persistent()
                    .get::<DataKey, soroban_sdk::Vec<Address>>(&DataKey::Roster(rid.clone()))
                {
                    snap.rosters.push((rid.clone(), r.iter().collect::<Vec<_>>()));
                }
                if let Some(m) = self
                    .env
                    .storage()
                    .persistent()
                    .get::<DataKey, bool>(&DataKey::Relay(rid.clone()))
                {
                    snap.relays.push((rid.clone(), m));
                }
            }
            snap
        })
    }

    /// Write a migrated snapshot into `dest`, standing in for the restored
    /// storage of the upgraded contract.
    pub fn restore(&self, dest: &Address, snap: &StorageSnapshot) {
        self.env.as_contract(dest, || {
            if let Some(admin) = &snap.admin {
                self.env.storage().instance().set(&DataKey::Admin, admin);
            }
            self.env
                .storage()
                .instance()
                .set(&DataKey::Schema, &snap.schema);
            for (rid, members) in &snap.rosters {
                let members_sv: soroban_sdk::Vec<Address> =
                    soroban_sdk::Vec::from_slice(&self.env, members.as_slice());
                self.env
                    .storage()
                    .persistent()
                    .set(&DataKey::Roster(rid.clone()), &members_sv);
                self.env
                    .storage()
                    .persistent()
                    .extend_ttl(&DataKey::Roster(rid.clone()), 100_000, 100_000);
            }
            for (rid, done) in &snap.relays {
                self.env
                    .storage()
                    .persistent()
                    .set(&DataKey::Relay(rid.clone()), done);
            }
            for (rid, addr, share) in &snap.members {
                self.env
                    .storage()
                    .persistent()
                    .set(&DataKey::Member(rid.clone(), addr.clone()), share);
            }
        })
    }

    /// Full upgrade simulation: snapshot `old`, transform the snapshot with
    /// `migration`, restore into a fresh v2 instance, and return it.
    pub fn upgrade_in_place(
        &self,
        old: Address,
        roster_ids: &[Bytes],
        migration: fn(&mut StorageSnapshot),
    ) -> Address {
        let mut snap = self.snapshot(&old, roster_ids);
        migration(&mut snap);
        let new_id = self.deploy_v2();
        self.restore(&new_id, &snap);
        new_id
    }
}

/// Schema-compatible migration: no key/layout changes, only bump the schema
/// version. v1-written records must remain directly readable.
#[allow(non_snake_case)]
pub fn migrate_SchemaCompatible(snap: &mut StorageSnapshot) {
    snap.schema = 2;
}

/// Schema-changing migration: equal-split each roster across its members and
/// mark the roster migrated — the sort of transform an on-chain `migrate_*`
/// entry point performs.
#[allow(non_snake_case)]
pub fn migrate_RosterToShares(snap: &mut StorageSnapshot) {
    snap.schema = 2;
    for (rid, members) in &snap.rosters {
        let count = members.len();
        let share = if count > 0 { crate::keys::SHARE_BASE / (count as u32) } else { 0 };
        for m in members {
            snap.members.push((rid.clone(), m.clone(), share));
        }
        snap.relays.push((rid.clone(), true));
    }
}

#[cfg(test)]
mod simulator_tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn snapshot_restore_round_trips() {
        let sim = UpgradeSimulator::new();
        sim.env.mock_all_auths();
        let id = sim.deploy_v1();
        let admin = Address::generate(&sim.env);
        crate::v1::V1Client::new(&sim.env, &id).initialize(&admin);

        let alice = Address::generate(&sim.env);
        let rid = Bytes::from_slice(&sim.env, b"roster-1");
        crate::v1::V1Client::new(&sim.env, &id).add_member(
            &rid,
            &soroban_sdk::vec![&sim.env, alice],
            &admin,
        );

        let snap = sim.snapshot(&id, &[rid.clone()]);
        assert_eq!(snap.rosters.len(), 1);
        assert_eq!(snap.schema, 1);

        let v2 = sim.upgrade_in_place(id, &[rid.clone()], migrate_SchemaCompatible);
        let roster = crate::v2::V2Client::new(&sim.env, &v2)
            .get_roster(&rid)
            .expect("roster preserved");
        assert_eq!(roster.len(), 1, "v1 records must survive the swap");
        assert_eq!(crate::v2::V2Client::new(&sim.env, &v2).storage_schema(), 2);
    }
}