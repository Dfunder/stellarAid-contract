//! Messaging contract — artist-client messaging with encryption metadata support.
//!
//! Closes #596 — Implement Messaging Contract.
//!
//! ## Features
//! - Encrypted message storage (client-side encryption; contract stores ciphertext)
//! - Message history retrieval per conversation
//! - Read receipts tracking
//! - Typing indicators
//! - Soft-delete (message body zeroed, `deleted` flag set)
//! - Rate limiting: 1 message per sender per `RATE_LIMIT_LEDGERS` per conversation

#![no_std]

mod errors;
mod types;

use soroban_sdk::{
    contract, contractimpl, symbol_short, Address, Bytes, Env, String, Vec,
};

use errors::MessagingError;
use types::{
    Conversation, DataKey, Message, ReadReceipt, TypingIndicator, MAX_CONV_ID_LEN,
    MAX_MESSAGE_LEN, MAX_HISTORY, RATE_LIMIT_LEDGERS,
};

#[contract]
pub struct MessagingContract;

#[contractimpl]
impl MessagingContract {
    // ── Lifecycle ─────────────────────────────────────────────────────────────

    /// Initialise the messaging contract with an admin address.
    /// Must be called once before any other operations.
    pub fn initialize(env: Env, admin: Address) -> Result<(), MessagingError> {
        admin.require_auth();
        if env.storage().instance().has(&DataKey::Initialized) {
            return Err(MessagingError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Initialized, &true);
        Ok(())
    }

    // ── Conversation management ───────────────────────────────────────────────

    /// Open a new conversation between `participant_a` and `participant_b`.
    ///
    /// Either participant may initiate. The conversation ID must be unique and
    /// is chosen by the caller (typically a deterministic hash of both
    /// addresses, e.g. `SHA256(sort([addr_a, addr_b]))`).
    ///
    /// Closes #596.
    pub fn create_conversation(
        env: Env,
        conv_id: Bytes,
        participant_a: Address,
        participant_b: Address,
    ) -> Result<(), MessagingError> {
        participant_a.require_auth();

        // ── Input validation ──────────────────────────────────────────────
        if conv_id.len() > MAX_CONV_ID_LEN {
            return Err(MessagingError::ConvIdTooLong);
        }

        let key = DataKey::Conversation(conv_id.clone());
        if env.storage().persistent().has(&key) {
            return Err(MessagingError::ConversationAlreadyExists);
        }

        let conv = Conversation {
            conv_id: conv_id.clone(),
            participant_a: participant_a.clone(),
            participant_b: participant_b.clone(),
            message_count: 0,
            created_ledger: env.ledger().sequence(),
        };
        env.storage().persistent().set(&key, &conv);

        env.events().publish(
            (symbol_short!("conv_new"),),
            (conv_id, participant_a, participant_b),
        );
        Ok(())
    }

    /// Retrieve a conversation by ID.
    pub fn get_conversation(env: Env, conv_id: Bytes) -> Result<Conversation, MessagingError> {
        env.storage()
            .persistent()
            .get(&DataKey::Conversation(conv_id))
            .ok_or(MessagingError::ConversationNotFound)
    }

    // ── Messaging ─────────────────────────────────────────────────────────────

    /// Send a message in an existing conversation.
    ///
    /// The `body` field should contain the encrypted ciphertext produced by
    /// client-side encryption. The contract stores it verbatim without
    /// inspecting plaintext content.
    ///
    /// Rate limiting: a sender may send at most 1 message per `RATE_LIMIT_LEDGERS`
    /// per conversation to prevent spam.
    ///
    /// Closes #596.
    pub fn send_message(
        env: Env,
        conv_id: Bytes,
        sender: Address,
        body: String,
    ) -> Result<u32, MessagingError> {
        sender.require_auth();

        // ── Input validation ──────────────────────────────────────────────
        if body.len() > MAX_MESSAGE_LEN {
            return Err(MessagingError::MessageTooLong);
        }

        // ── Load conversation and verify sender is a participant ───────────
        let mut conv: Conversation = env.storage()
            .persistent()
            .get(&DataKey::Conversation(conv_id.clone()))
            .ok_or(MessagingError::ConversationNotFound)?;

        if sender != conv.participant_a && sender != conv.participant_b {
            return Err(MessagingError::Unauthorized);
        }

        // ── Rate limiting (closes #596) ────────────────────────────────────
        let rate_key = DataKey::LastSendLedger(conv_id.clone(), sender.clone());
        let last_send: u32 = env.storage().instance().get(&rate_key).unwrap_or(0);
        let current_ledger = env.ledger().sequence();
        if current_ledger < last_send.saturating_add(RATE_LIMIT_LEDGERS) {
            return Err(MessagingError::RateLimitExceeded);
        }
        env.storage().instance().set(&rate_key, &current_ledger);

        // ── Determine sequence number ──────────────────────────────────────
        conv.message_count = conv.message_count.saturating_add(1);
        let seq = conv.message_count;

        // Generate a deterministic message_id from conv_id bytes + seq
        let mut id_bytes = Bytes::new(&env);
        id_bytes.append(&conv_id);
        // Append seq as 4 big-endian bytes
        let seq_bytes = [
            ((seq >> 24) & 0xff) as u8,
            ((seq >> 16) & 0xff) as u8,
            ((seq >> 8) & 0xff) as u8,
            (seq & 0xff) as u8,
        ];
        id_bytes.push_back(seq_bytes[0]);
        id_bytes.push_back(seq_bytes[1]);
        id_bytes.push_back(seq_bytes[2]);
        id_bytes.push_back(seq_bytes[3]);

        let msg = Message {
            message_id: id_bytes,
            conv_id: conv_id.clone(),
            sender: sender.clone(),
            body,
            sent_ledger: current_ledger,
            deleted: false,
        };

        // ── Persist ───────────────────────────────────────────────────────
        env.storage().persistent().set(&DataKey::Message(conv_id.clone(), seq), &msg);
        env.storage().persistent().set(&DataKey::Conversation(conv_id.clone()), &conv);

        // ── Event ─────────────────────────────────────────────────────────
        env.events().publish(
            (symbol_short!("msg_sent"),),
            (conv_id, sender, seq),
        );

        Ok(seq)
    }

    /// Retrieve a page of messages for a conversation.
    ///
    /// Returns messages `[from_seq, from_seq + limit)` (1-based).
    /// Caller must be a participant.
    ///
    /// Closes #596.
    pub fn get_messages(
        env: Env,
        conv_id: Bytes,
        caller: Address,
        from_seq: u32,
        limit: u32,
    ) -> Result<Vec<Message>, MessagingError> {
        let conv: Conversation = env.storage()
            .persistent()
            .get(&DataKey::Conversation(conv_id.clone()))
            .ok_or(MessagingError::ConversationNotFound)?;

        if caller != conv.participant_a && caller != conv.participant_b {
            return Err(MessagingError::Unauthorized);
        }

        let cap = limit.min(MAX_HISTORY);
        let mut result: Vec<Message> = Vec::new(&env);
        let end_seq = from_seq.saturating_add(cap).min(conv.message_count + 1);
        let mut seq = from_seq;
        while seq < end_seq {
            if let Some(msg) = env.storage()
                .persistent()
                .get::<DataKey, Message>(&DataKey::Message(conv_id.clone(), seq))
            {
                result.push_back(msg);
            }
            seq = seq.saturating_add(1);
        }
        Ok(result)
    }

    // ── Soft delete ───────────────────────────────────────────────────────────

    /// Soft-delete a message. The body is replaced with an empty string and
    /// the `deleted` flag is set to `true`. Only the original sender may
    /// delete their own messages.
    ///
    /// Closes #596.
    pub fn delete_message(
        env: Env,
        conv_id: Bytes,
        seq: u32,
        caller: Address,
    ) -> Result<(), MessagingError> {
        caller.require_auth();

        let mut msg: Message = env.storage()
            .persistent()
            .get(&DataKey::Message(conv_id.clone(), seq))
            .ok_or(MessagingError::MessageNotFound)?;

        if msg.deleted {
            return Err(MessagingError::AlreadyDeleted);
        }
        if msg.sender != caller {
            return Err(MessagingError::CannotDeleteOthers);
        }

        msg.deleted = true;
        msg.body = String::from_str(&env, "");
        env.storage().persistent().set(&DataKey::Message(conv_id.clone(), seq), &msg);

        env.events().publish(
            (symbol_short!("msg_del"),),
            (conv_id, seq, caller),
        );
        Ok(())
    }

    // ── Read receipts ─────────────────────────────────────────────────────────

    /// Mark messages up to `up_to_seq` as read by `reader`.
    /// Reader must be a participant in the conversation.
    ///
    /// Closes #596.
    pub fn mark_read(
        env: Env,
        conv_id: Bytes,
        reader: Address,
        up_to_seq: u32,
    ) -> Result<(), MessagingError> {
        reader.require_auth();

        let conv: Conversation = env.storage()
            .persistent()
            .get(&DataKey::Conversation(conv_id.clone()))
            .ok_or(MessagingError::ConversationNotFound)?;

        if reader != conv.participant_a && reader != conv.participant_b {
            return Err(MessagingError::Unauthorized);
        }

        let receipt = ReadReceipt {
            conv_id: conv_id.clone(),
            reader: reader.clone(),
            last_read_seq: up_to_seq,
            updated_ledger: env.ledger().sequence(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::ReadReceipt(conv_id.clone(), reader.clone()), &receipt);

        env.events().publish(
            (symbol_short!("msg_read"),),
            (conv_id, reader, up_to_seq),
        );
        Ok(())
    }

    /// Retrieve the read receipt for a participant in a conversation.
    ///
    /// Closes #596.
    pub fn get_read_receipt(
        env: Env,
        conv_id: Bytes,
        reader: Address,
    ) -> Option<ReadReceipt> {
        env.storage()
            .persistent()
            .get(&DataKey::ReadReceipt(conv_id, reader))
    }

    // ── Typing indicators ─────────────────────────────────────────────────────

    /// Set a typing indicator for `typer` in `conv_id`.
    ///
    /// The indicator expires after ~30 ledgers (~2.5 minutes at 5 s/ledger).
    /// Callers should check `set_ledger + 30 >= current_ledger` to determine
    /// whether the indicator is still active.
    ///
    /// Closes #596.
    pub fn set_typing(
        env: Env,
        conv_id: Bytes,
        typer: Address,
    ) -> Result<(), MessagingError> {
        typer.require_auth();

        let conv: Conversation = env.storage()
            .persistent()
            .get(&DataKey::Conversation(conv_id.clone()))
            .ok_or(MessagingError::ConversationNotFound)?;

        if typer != conv.participant_a && typer != conv.participant_b {
            return Err(MessagingError::Unauthorized);
        }

        let indicator = TypingIndicator {
            conv_id: conv_id.clone(),
            typer: typer.clone(),
            set_ledger: env.ledger().sequence(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::TypingIndicator(conv_id.clone(), typer.clone()), &indicator);

        env.events().publish(
            (symbol_short!("typing"),),
            (conv_id, typer),
        );
        Ok(())
    }

    /// Retrieve the typing indicator for a participant, if any.
    ///
    /// Returns `None` if no indicator is stored or if it has expired
    /// (set_ledger + 30 < current_ledger).
    ///
    /// Closes #596.
    pub fn get_typing(env: Env, conv_id: Bytes, typer: Address) -> Option<TypingIndicator> {
        const TYPING_EXPIRY_LEDGERS: u32 = 30;
        let indicator: Option<TypingIndicator> = env.storage()
            .persistent()
            .get(&DataKey::TypingIndicator(conv_id, typer));
        indicator.and_then(|ind| {
            if ind.set_ledger.saturating_add(TYPING_EXPIRY_LEDGERS) >= env.ledger().sequence() {
                Some(ind)
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env, String};

    #[test]
    fn test_create_conversation_and_send_message() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, MessagingContract);
        let client = MessagingContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        client.initialize(&admin);

        let conv_id = soroban_sdk::Bytes::from_slice(&env, b"conv-alice-bob");
        client.create_conversation(&conv_id, &alice, &bob);

        let conv = client.get_conversation(&conv_id);
        assert_eq!(conv.participant_a, alice);
        assert_eq!(conv.participant_b, bob);
        assert_eq!(conv.message_count, 0);

        // Jump ahead past the rate limit window
        env.ledger().with_mut(|l| l.sequence_number = RATE_LIMIT_LEDGERS + 1);

        let body = String::from_str(&env, "Hello Bob!");
        let seq = client.send_message(&conv_id, &alice, &body);
        assert_eq!(seq, 1);

        let messages = client.get_messages(&conv_id, &alice, &1, &10);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages.get(0).unwrap().body, body);
    }

    #[test]
    fn test_read_receipts() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, MessagingContract);
        let client = MessagingContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        client.initialize(&admin);
        let conv_id = soroban_sdk::Bytes::from_slice(&env, b"conv-receipts");
        client.create_conversation(&conv_id, &alice, &bob);

        // Send a message
        env.ledger().with_mut(|l| l.sequence_number = RATE_LIMIT_LEDGERS + 1);
        let body = String::from_str(&env, "hi");
        client.send_message(&conv_id, &alice, &body);

        // Bob marks it read
        client.mark_read(&conv_id, &bob, &1);
        let receipt = client.get_read_receipt(&conv_id, &bob);
        assert!(receipt.is_some());
        assert_eq!(receipt.unwrap().last_read_seq, 1);
    }

    #[test]
    fn test_soft_delete() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, MessagingContract);
        let client = MessagingContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        client.initialize(&admin);
        let conv_id = soroban_sdk::Bytes::from_slice(&env, b"conv-delete");
        client.create_conversation(&conv_id, &alice, &bob);

        env.ledger().with_mut(|l| l.sequence_number = RATE_LIMIT_LEDGERS + 1);
        let body = String::from_str(&env, "delete me");
        client.send_message(&conv_id, &alice, &body);

        client.delete_message(&conv_id, &1, &alice);

        let messages = client.get_messages(&conv_id, &alice, &1, &1);
        assert!(messages.get(0).unwrap().deleted);
        assert_eq!(messages.get(0).unwrap().body, String::from_str(&env, ""));
    }

    #[test]
    fn test_rate_limit() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, MessagingContract);
        let client = MessagingContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        client.initialize(&admin);
        let conv_id = soroban_sdk::Bytes::from_slice(&env, b"conv-ratelimit");
        client.create_conversation(&conv_id, &alice, &bob);

        env.ledger().with_mut(|l| l.sequence_number = RATE_LIMIT_LEDGERS + 1);
        let body = String::from_str(&env, "first");
        client.send_message(&conv_id, &alice, &body);

        // Second send in the same ledger window should be rate limited
        let result = client.try_send_message(&conv_id, &alice, &String::from_str(&env, "second"));
        assert!(result.is_err());
    }

    #[test]
    fn test_typing_indicator() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, MessagingContract);
        let client = MessagingContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        client.initialize(&admin);
        let conv_id = soroban_sdk::Bytes::from_slice(&env, b"conv-typing");
        client.create_conversation(&conv_id, &alice, &bob);

        client.set_typing(&conv_id, &alice);
        let indicator = client.get_typing(&conv_id, &alice);
        assert!(indicator.is_some());
    }

    #[test]
    fn test_unauthorized_participant() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, MessagingContract);
        let client = MessagingContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let charlie = Address::generate(&env);

        client.initialize(&admin);
        let conv_id = soroban_sdk::Bytes::from_slice(&env, b"conv-unauth");
        client.create_conversation(&conv_id, &alice, &bob);

        // Charlie tries to send a message — should fail
        env.ledger().with_mut(|l| l.sequence_number = RATE_LIMIT_LEDGERS + 1);
        let result = client.try_send_message(&conv_id, &charlie, &String::from_str(&env, "hack"));
        assert!(result.is_err());
    }
}
