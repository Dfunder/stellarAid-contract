//! Data types for the Messaging contract (closes #596).

use soroban_sdk::{contracttype, Address, Bytes, String};

// ── Limits ───────────────────────────────────────────────────────────────────

/// Maximum byte length for a message body / encrypted payload.
pub const MAX_MESSAGE_LEN: u32 = 4096;
/// Maximum byte length for a conversation identifier.
pub const MAX_CONV_ID_LEN: u32 = 64;
/// Maximum byte length for a message identifier.
pub const MAX_MSG_ID_LEN: u32 = 64;
/// Maximum number of messages kept per conversation before the oldest is pruned.
pub const MAX_HISTORY: u32 = 100;
/// Minimum ledgers between messages from the same sender (rate limit).
/// At ~5 s/ledger: 12 ledgers ≈ 1 minute.
pub const RATE_LIMIT_LEDGERS: u32 = 12;

// ── Types ────────────────────────────────────────────────────────────────────

/// A stored message. The `body` field holds the (optionally encrypted) content.
/// The contract stores only the ciphertext / metadata; encryption is done
/// client-side before calling `send_message`.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Message {
    /// Unique message identifier (deterministic: hash of conv_id + sender + seq).
    pub message_id: Bytes,
    /// Conversation this message belongs to.
    pub conv_id: Bytes,
    /// Sender address.
    pub sender: Address,
    /// Message body / encrypted payload.
    pub body: String,
    /// Ledger sequence at which the message was sent.
    pub sent_ledger: u32,
    /// Whether this message has been soft-deleted.
    pub deleted: bool,
}

/// A conversation between exactly two parties.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Conversation {
    pub conv_id: Bytes,
    pub participant_a: Address,
    pub participant_b: Address,
    /// Total message count (used to generate sequential message IDs).
    pub message_count: u32,
    /// Ledger at which the conversation was created.
    pub created_ledger: u32,
}

/// Read-receipt record: tracks the last message sequence read by each party.
#[contracttype]
#[derive(Clone, Debug)]
pub struct ReadReceipt {
    pub conv_id: Bytes,
    pub reader: Address,
    /// The message sequence number (1-based) up to which the reader has read.
    pub last_read_seq: u32,
    pub updated_ledger: u32,
}

/// Typing indicator record.
#[contracttype]
#[derive(Clone, Debug)]
pub struct TypingIndicator {
    pub conv_id: Bytes,
    pub typer: Address,
    /// Ledger at which the typing indicator was set. Expires after ~30 ledgers.
    pub set_ledger: u32,
}

// ── Storage keys ─────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    /// Initialized flag.
    Initialized,
    /// Admin address.
    Admin,
    /// Conversation record.
    Conversation(Bytes),
    /// Individual message: (conv_id, sequence_number).
    Message(Bytes, u32),
    /// Read receipt: (conv_id, reader_address).
    ReadReceipt(Bytes, Address),
    /// Typing indicator: (conv_id, typer_address).
    TypingIndicator(Bytes, Address),
    /// Rate limit: last send ledger for (conv_id, sender).
    LastSendLedger(Bytes, Address),
}
