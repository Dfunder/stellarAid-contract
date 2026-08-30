//! v1 mock contract: representative current-deployed behaviour.
//!
//! Mirrors the upgrade surface of a real workspace contract: `initialize`,
//! version queries, and an admin-gated write path, all backed by the shared
//! [`DataKey`] layout.

use soroban_sdk::{contract, contractimpl, Address, Bytes, Env, Vec};

use crate::keys::DataKey;

#[contract]
pub struct V1;

#[contractimpl]
impl V1 {
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Schema, &1u32);
    }

    pub fn storage_schema(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::Schema).unwrap_or(0)
    }

    /// Reports the deployed version tuple, mirroring `get_version`.
    pub fn version(env: Env) -> (u32, u32, u32) {
        let _ = env;
        (1, 0, 0)
    }

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

    pub fn get_roster(env: Env, roster_id: Bytes) -> Option<Vec<Address>> {
        env.storage().persistent().get(&DataKey::Roster(roster_id))
    }

    /// Admin read helper used by the simulator to enumerate rosters (kept for
    /// regression parity with the v2 API).
    pub fn admin(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    }
}