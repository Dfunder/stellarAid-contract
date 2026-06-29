#![no_std]
use soroban_sdk::{contracttype, Address, Symbol};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    ActiveSession(Address), // Maps a user profile to their current active signing wallet
    SessionNonce(Address),  // Tracks transaction replay prevention nonces per wallet
    IsFlowLocked(Address),   // Mid-flow guard: locks a wallet if an action is incomplete
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionProfile {
    pub wallet_address: Address,
    pub network_passphrase: Symbol, // Re-verifies network safety (e.g., TESTNET vs PUBLIC)
    pub last_active_ledger: u32,
}