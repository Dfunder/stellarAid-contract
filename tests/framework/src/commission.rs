//! A minimal commission-agreement stand-in used by integration scenarios.
//!
//! The real `commission_agreement` contract (PR #657/#658) will eventually
//! expose `get_agreement_escrow_amount`; this stub mirrors that same interface
//! so the atomic escrow→commission flow (#656) can be exercised end-to-end.

use soroban_sdk::{contract, contractimpl, Bytes, Env};

#[contract]
pub struct CommissionStub;

#[contractimpl]
impl CommissionStub {
    pub fn init(env: Env, expected_amount: i128) {
        env.storage().instance().set(&0u32, &expected_amount);
    }

    pub fn get_agreement_escrow_amount(env: Env, _commission_id: Bytes) -> i128 {
        env.storage().instance().get(&0u32).unwrap()
    }
}