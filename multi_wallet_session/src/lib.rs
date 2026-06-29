#![no_std]
use soroban_sdk::{contractimpl, log, Address, Env, Symbol};

mod data_types;
use crate::data_types::{DataKey, SessionProfile};

pub struct MultiWalletSessionContract;

#[contractimpl]
impl MultiWalletSessionContract {
    /// Connects or switches to a new wallet address for an active on-chain session state wrapper.
    pub fn switch_wallet(
        env: Env, 
        user: Address, 
        new_wallet: Address, 
        network: Symbol
    ) {
        // Strict Task Validation: Ensure the transaction call is authentically signed by the target wallet
        new_wallet.require_auth();

        // 1. Mid-flow Guard Check: If the previous wallet was locked mid-transaction (e.g., incomplete multi-sig split escrow)
        if env.storage().persistent().has(&DataKey::IsFlowLocked(user.clone())) {
            let is_locked: bool = env.storage().persistent().get(&DataKey::IsFlowLocked(user.clone())).unwrap();
            if is_locked {
                log!(&env, "CRITICAL ERROR: Cannot switch wallets. Active transaction pipeline is locked mid-flow.");
                panic!("Session is currently pipeline-locked. Complete or revert active flows first.");
            }
        }

        // 2. Clear previous wallet-specific states (State Isolation Cleanup)
        if env.storage().persistent().has(&DataKey::ActiveSession(user.clone())) {
            env.storage().persistent().remove(&DataKey::ActiveSession(user.clone()));
            log!(&env, "Previous wallet session purged successfully.");
        }

        // 3. Network Re-verification: Enforce network boundary rules
        let expected_network = Symbol::new(&env, "testnet");
        if network != expected_network {
            panic!("Network verification handshake failed. Target network passphrase misaligned.");
        }

        // 4. Save the new isolated structural session mapping data profile
        let session = SessionProfile {
            wallet_address: new_wallet.clone(),
            network_passphrase: network,
            last_active_ledger: env.ledger().sequence(),
        };

        env.storage().persistent().set(&DataKey::ActiveSession(user), &session);
        log!(&env, "Successfully switched session execution context to new wallet address.");
    }

    /// Explicitly locks or unlocks a session state during high-risk multi-stage pipelines (e.g., donation pool splits)
    pub fn set_flow_lock(env: Env, user: Address, wallet: Address, lock_state: bool) {
        wallet.require_auth();
        
        // Assert that the request vector aligns perfectly with the logged session data instance
        if let Some(session) = env.storage().persistent().get::<_, SessionProfile>(&DataKey::ActiveSession(user.clone())) {
            if session.wallet_address != wallet {
                panic!("Mismatched executing wallet validation parameters.");
            }
        } else {
            panic!("No active wallet connection session found for user.");
        }

        env.storage().persistent().set(&DataKey::IsFlowLocked(user), &lock_state);
    }

    /// Clear all active sessions explicitly (Disconnect Workflow)
    pub fn disconnect_wallet(env: Env, user: Address, wallet: Address) {
        wallet.require_auth();
        
        env.storage().persistent().remove(&DataKey::ActiveSession(user.clone()));
        env.storage().persistent().remove(&DataKey::IsFlowLocked(user));
        log!(&env, "Session dropped cleanly. All volatile storage flags cleared.");
    }
}