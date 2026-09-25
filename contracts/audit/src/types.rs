//! Types for the transaction history and audit log contract (closes #712).

use soroban_sdk::{contracttype, Address, Bytes, Vec};

/// What kind of transition an [`AuditEntry`] records.
///
/// A closed vocabulary rather than a caller-supplied string, for three reasons:
/// the log cannot be used to smuggle free-form user content onto the chain, a
/// stored entry's size is bounded, and indexers get a stable set of labels to
/// match on.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditAction {
    EscrowCreated = 0,
    EscrowReleased = 1,
    EscrowRefunded = 2,
    EscrowDisputed = 3,
    CommissionCreated = 4,
    MilestoneApproved = 5,
    DisputeOpened = 6,
    DisputeResolved = 7,
    DonationReceived = 8,
    WithdrawalRequested = 9,
    WithdrawalSettled = 10,
    /// A self-attested user activity marker. Carries no value movement, so
    /// `amount` is `0` and `token` is `None`.
    UserActivity = 11,
}

/// Where the value a transaction entry describes currently stands.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TxStatus {
    /// Value is held and not yet settled.
    Pending = 0,
    /// Settled to the recipient.
    Completed = 1,
    /// Attempted and failed; the value did not move.
    Failed = 2,
    /// Returned to the sender.
    Refunded = 3,
    /// Abandoned before settlement.
    Cancelled = 4,
    /// A user-activity marker, not a value transfer.
    Activity = 5,
}

/// One immutable line of the log.
///
/// Everything needed to answer "what happened, when, to whom, for how much" is
/// carried inline: the ledger plus its timestamp is the ordering key, and
/// `from` / `to` are the parties.
///
/// A transaction that changes status does **not** rewrite its earlier entry. The
/// new state is appended as a further entry with the same `reference`, so the
/// full trail survives and `get_transaction` returns the newest one.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditEntry {
    /// Monotonic, never-reused append number. This is the entry's primary key
    /// and the reason an existing entry can never be overwritten.
    pub sequence: u32,
    /// Opaque id of the business object this entry describes — a commission id,
    /// an escrow id, a withdrawal id. Repeated across the status trail.
    pub reference: Bytes,
    pub action: AuditAction,
    /// Initiator / source of the value.
    pub from: Address,
    /// Recipient / destination of the value.
    pub to: Address,
    /// Amount in the token's smallest unit (stroops of the base asset) — the same
    /// `i128` representation the rest of the workspace uses. Never a float, and
    /// never negative: a negative amount has no meaning in an audit record.
    pub amount: i128,
    /// Token the amount is denominated in; `None` only for
    /// [`AuditAction::UserActivity`].
    pub token: Option<Address>,
    pub status: TxStatus,
    /// Ledger at which the entry was appended.
    pub ledger: u32,
    /// Unix timestamp of that ledger, so an auditor can range on dates without
    /// keeping the whole chain locally.
    pub timestamp: u64,
}

/// One bounded page of query results.
///
/// `next_sequence` is the cursor to resume a bounded walk from. It is `None`
/// when the walk reached the end of the underlying index or scan window.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditPage {
    pub entries: Vec<AuditEntry>,
    /// Zero-based page index that was served.
    pub page: u32,
    /// Page size that was served, after clamping.
    pub page_size: u32,
    /// Matches found inside the window this call examined.
    pub total: u32,
    /// `true` when matches exist beyond this page.
    pub has_more: bool,
    /// `true` when the walk stopped early at the query bound and did **not**
    /// cover the whole window. A consumer must treat a truncated page as
    /// partial — this is the flag that says so.
    pub truncated: bool,
    /// Cursor to continue from, or `None` when the walk finished.
    pub next_sequence: Option<u32>,
}

/// An export of a ledger window, for producing an audit report.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditExport {
    pub from_ledger: u32,
    pub to_ledger: u32,
    pub entries: Vec<AuditEntry>,
    /// Appends that fell inside the requested window and were reached.
    pub total: u32,
    /// `true` when the walk hit the scan bound before reaching `to_ledger`, so
    /// the report is a partial view of the window.
    pub truncated: bool,
}
