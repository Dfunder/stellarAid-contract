//! A legacy-interface stand-in for the "creator config" contract.
//!
//! The escrow contract consumes its injected config through the
//! `get_fee_b` / `get_usdc` / `get_adm` / `get_pw` interface. The registry-first
//! implementation (PR #662) exposes `resolve_for_environment` instead, so the
//! framework deploys this stub to keep the classic escrow lifecycle testable
//! while `platform_config` itself is exercised through its own registry API.

use soroban_sdk::{contract, contractimpl, Address, Env};

#[contract]
pub struct ConfigStub;

#[contractimpl]
impl ConfigStub {
    pub fn init(env: Env, fee_bps: u32, usdc: Address, admin: Address, platform_wallet: Address) {
        env.storage().instance().set(&0u32, &fee_bps);
        env.storage().instance().set(&1u32, &usdc);
        env.storage().instance().set(&2u32, &admin);
        env.storage().instance().set(&3u32, &platform_wallet);
    }

    pub fn get_fee_b(env: Env) -> u32 {
        env.storage().instance().get(&0u32).unwrap()
    }

    pub fn get_usdc(env: Env) -> Address {
        env.storage().instance().get(&1u32).unwrap()
    }

    pub fn get_adm(env: Env) -> Address {
        env.storage().instance().get(&2u32).unwrap()
    }

    pub fn get_pw(env: Env) -> Address {
        env.storage().instance().get(&3u32).unwrap()
    }
}