//! Shared dispute-timing constants (closes #977).
//!
//! `escrow` and `dispute_arbiter` both bound how long a dispute may run. A
//! single definition here is what stops the two contracts from drifting into
//! independent timers — exactly the coordination bug #977 describes. Both
//! crates source their TTL default and ceiling from these values, so a
//! deadline computed by one is the same ledger the other reports.

/// Default dispute TTL: ~60 days at 6 s/ledger. Roughly twice the base escrow
/// TTL so an active dispute never races record expiration.
pub const DEFAULT_DISPUTE_TTL_LEDGERS: u32 = 864_000;

/// Longest dispute TTL an admin may configure: ~180 days at 6 s/ledger.
pub const MAX_DISPUTE_TTL_LEDGERS: u32 = 2_592_000;
