#![no_std]
use soroban_sdk::{contract, contractimpl, log, Address, Env};

mod data_types;
use crate::data_types::{DataKey, DelayedState};

#[contract]
pub struct WalletTimeoutContract;

#[contractimpl]
impl WalletTimeoutContract {
    /// Pre-authorizes an intent ledger checkpoint. 
    /// If the matching signature execution block fails to materialize inside the deadline, it allows retries.
    pub fn initiate_action(env: Env, user: Address) {
        user.require_auth();

        // Clear any previous dangling state if it exists, allowing clean retry loops
        if env.storage().temporary().has(&DataKey::PendingExecution(user.clone())) {
            env.storage().temporary().remove(&DataKey::PendingExecution(user.clone()));
        }

        let state = DelayedState {
            value: 1, // Represents baseline transaction configuration metadata
            initiator: user.clone(),
        };

        // Use temporary storage so expired, un-executed inputs naturally decay from the ledger state
        env.storage().temporary().set(&DataKey::PendingExecution(user), &state);
        log!(&env, "Action state recorded. Awaiting final signature verification.");
    }

    /// Completes the execution loop, guarded strictly by a maximum ledger sequence threshold.
    /// In Soroban, a 60-second window roughly equates to an offset of 12 ledger blocks (~5s per ledger closing).
    pub fn execute_with_timeout(
        env: Env, 
        user: Address, 
        max_ledger_sequence: u32
    ) {
        user.require_auth();

        // 1. Evaluate Timeout Condition: Check if current ledger height exceeds the client-side signature boundary
        let current_ledger = env.ledger().sequence();
        if current_ledger > max_ledger_sequence {
            // Revert state change. This prevents orphaned pending structures from freezing the user flow.
            env.storage().temporary().remove(&DataKey::PendingExecution(user.clone()));
            log!(&env, "CRITICAL: Wallet signature collection window exceeded standard timeout limits.");
            panic!("Signing timed out. Transaction window has expired. Please retry the operation.");
        }

        // 2. Fetch and confirm matching state primitives exist
        if !env.storage().temporary().has(&DataKey::PendingExecution(user.clone())) {
            panic!("No pending action matches this execution parameter block or state was dropped.");
        }

        // 3. Complete the execution pipeline safely
        env.storage().temporary().remove(&DataKey::PendingExecution(user));
        log!(&env, "Signature validated within deadline constraints. Pipeline executed successfully.");
    }
}