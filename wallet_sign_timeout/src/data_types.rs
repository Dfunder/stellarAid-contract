use soroban_sdk::{contracttype, Address};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    PendingExecution(Address), // Tracks active execution states to prevent orphaned data pools
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DelayedState {
    pub value: u32,
    pub initiator: Address,
}
