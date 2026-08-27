use soroban_sdk::{contracttype, Bytes, String};

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisputeStatus {
    Open = 0,
    ResolvedForClient = 1,
    ResolvedForArtist = 2,
    PartiallyResolved = 3,
    AutoResolved = 4,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeRecord {
    pub commission_id: Bytes,
    pub opened_ledger: u32,
    pub auto_resolve_ledger: u32,
    pub status: DisputeStatus,
    pub resolution_note: Option<String>,
}

#[contracttype]
pub enum DataKey {
    Admin,
    EscrowContract,
    ConfigContract,
    Dispute(Bytes),
    AutoResolveLedgers,
}
