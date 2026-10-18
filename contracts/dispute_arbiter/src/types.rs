use soroban_sdk::{contracttype, Address, Bytes, String};

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
    /// Ledger from which no further evidence may be submitted and at which
    /// `auto_resolve` becomes callable (#977).
    ///
    /// Set once, when the dispute is opened, from the single configurable
    /// dispute TTL ([`shared::dispute::DEFAULT_DISPUTE_TTL_LEDGERS`]) so the
    /// evidence window and the auto-resolve timeout are one concept rather
    /// than two independent timers.
    pub deadline_ledger: u32,
    pub status: DisputeStatus,
    pub resolution_note: Option<String>,
}

/// One piece of evidence attached to a dispute (#977).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceRecord {
    pub commission_id: Bytes,
    pub submitter: Address,
    pub evidence: Bytes,
    pub submitted_ledger: u32,
}

#[contracttype]
pub enum DataKey {
    Admin,
    EscrowContract,
    ConfigContract,
    Dispute(Bytes),
    /// Configurable dispute TTL in ledgers (#977).
    DisputeTtlLedgers,
    /// One evidence entry.  Key: (commission id, index).
    Evidence(Bytes, u32),
    /// Number of evidence entries recorded for a dispute.
    EvidenceCount(Bytes),
}
