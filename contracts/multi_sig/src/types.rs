//! Types for the multi-signature authorization contract (closes #709).

use soroban_sdk::{contracttype, Address, Bytes, Vec};

/// Largest signer set a single configuration may hold. Keeps the signer
/// membership check inside a predictable budget.
pub const MAX_SIGNERS: u32 = 10;
/// A threshold below this is not a multi-signature scheme at all, so it is
/// rejected at configuration time rather than producing a one-of-N contract
/// that looks like a 2-of-3.
pub const MIN_THRESHOLD: u32 = 2;
/// Suggested signature lifetime: ~24 h at 5 s/ledger.
pub const DEFAULT_EXPIRY_LEDGERS: u32 = 17_280;
/// Longest lifetime an admin may configure: ~7 days at 5 s/ledger.
pub const MAX_EXPIRY_LEDGERS: u32 = 120_960;

/// Who may sign, how many of them must sign, and how long a collected set stays
/// valid.
///
/// `2-of-3` and `3-of-5` are both just `{ signers: [a, b, c], threshold: 2 }`
/// and `{ signers: [a, b, c, d, e], threshold: 3 }`; nothing in the state
/// machine is specialised to either shape.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultiSigConfig {
    /// The authorised signers, in configuration order and guaranteed free of
    /// duplicates.
    pub signers: Vec<Address>,
    /// How many distinct signers must approve before a proposal is approved.
    pub threshold: u32,
    /// How long a proposal's collected signatures stay valid.
    pub expiry_ledgers: u32,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProposalStatus {
    /// Open for signatures, threshold not reached yet.
    Pending = 0,
    /// Threshold reached and `finalize` confirmed it.
    Approved = 1,
    /// The signature window elapsed before the threshold was reached.
    Expired = 2,
}

/// One action awaiting authorisation.
///
/// `action` is an opaque payload chosen by the integrating contract. This crate
/// never interprets it — it only guarantees that the approvals attached to a
/// proposal were authorised by the signers for *this* proposal id, and that
/// the set reaching `threshold` is what `finalize` observes.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Proposal {
    pub id: Bytes,
    /// Opaque action payload; see the module docs.
    pub action: Bytes,
    /// Who opened the proposal. Opening requires the creator's own auth, so a
    /// proposal cannot be attributed to someone who did not ask for it.
    pub creator: Address,
    pub status: ProposalStatus,
    /// Distinct authorised signers that have approved so far.
    pub signature_count: u32,
    pub created_ledger: u32,
    /// Ledger from which the proposal can no longer be signed.
    pub expires_ledger: u32,
}

/// Storage keys.
#[contracttype]
pub enum DataKey {
    /// Contract admin, the only address that may rewrite the configuration.
    Admin,
    /// The current [`MultiSigConfig`].
    Config,
    /// Open proposal.  Key: proposal id.
    Proposal(Bytes),
    /// One collected signature.  Key: (proposal id, signer). The value is the
    /// ledger the signature landed on. Never removed: a signature is valid for
    /// the life of the proposal and cannot be withdrawn.
    Approval(Bytes, Address),
    /// Running count of proposals ever opened.
    ProposalCount,
}
