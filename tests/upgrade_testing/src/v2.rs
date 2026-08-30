//! v2 mock contract: the upgraded WASM after deploying new release behaviour.
//!
//! Keeps the entire v1 surface (same instances keys, same `Roster` layout) so
//! v1-written records deserialize unchanged (backward compatibility), and adds
//! a migration entry point that computes per-member shares and marks rosters as
//! migrated.

use soroban_sdk::{contract, contractimpl, Address, Bytes, Env, Vec};

use crate::keys::{DataKey, SHARE_BASE};

#[contract]
pub struct V2;

#[contractimpl]
impl V2 {
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Schema, &2u32);
    }

    pub fn storage_schema(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::Schema).unwrap_or(0)
    }

    /// Reports the deployed version tuple, mirroring `get_version`.
    pub fn version(env: Env) -> (u32, u32, u32) {
        let _ = env;
        (2, 0, 0)
    }

    /// v1-compatible write path: identical key + layout, so an un-upgraded
    /// roster written by v1 is directly readable (backward compatibility).
    pub fn add_member(env: Env, roster_id: Bytes, members: Vec<Address>, admin: Address) {
        admin.require_auth();
        let stored: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if stored != admin {
            panic!("unauthorized");
        }
        env.storage()
            .persistent()
            .set(&DataKey::Roster(roster_id.clone()), &members);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Roster(roster_id), 100_000, 100_000);
    }

    /// v1-compatible read path. Must keep working after an upgrade.
    pub fn get_roster(env: Env, roster_id: Bytes) -> Option<Vec<Address>> {
        env.storage().persistent().get(&DataKey::Roster(roster_id))
    }

    /// Migration: equal-split the roster across members, record shares, mark
    /// the roster migrated. Idempotent — a second call is a no-op and reports
    /// the roster as migrated (`true`). Panics if the roster does not exist.
    pub fn migrate(env: Env, admin: Address, roster_id: Bytes) -> bool {
        let roster: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::Roster(roster_id.clone()))
            .expect("roster not found");
        if env
            .storage()
            .persistent()
            .get(&DataKey::Relay(roster_id.clone()))
            .unwrap_or(false)
        {
            return true;
        }
        let count = roster.len();
        let share = if count > 0 { SHARE_BASE / count } else { 0 };
        let mut i = 0u32;
        while i < count {
            let member = roster.get(i).unwrap();
            env.storage()
                .persistent()
                .set(&DataKey::Member(roster_id.clone(), member), &share);
            i += 1;
        }
        env.storage()
            .persistent()
            .set(&DataKey::Relay(roster_id.clone()), &true);
        let _ = admin;
        true
    }

    pub fn get_member(env: Env, roster_id: Bytes, member: Address) -> Option<u32> {
        env.storage()
            .persistent()
            .get(&DataKey::Member(roster_id, member))
    }

    pub fn is_migrated(env: Env, roster_id: Bytes) -> bool {
        env.storage().persistent().get(&DataKey::Relay(roster_id)).unwrap_or(false)
    }

    pub fn admin(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    }
}