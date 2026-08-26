//! Errors for the Messaging contract (closes #596).

use soroban_sdk::{contracterror, symbol_short, Symbol};

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessagingError {
    /// Contract has already been initialized.
    AlreadyInitialized = 1,
    /// Conversation not found.
    ConversationNotFound = 2,
    /// Message not found.
    MessageNotFound = 3,
    /// Caller is not a participant in the conversation.
    Unauthorized = 4,
    /// Message body exceeds the maximum allowed length.
    MessageTooLong = 5,
    /// Conversation ID exceeds the maximum allowed length.
    ConvIdTooLong = 6,
    /// Message ID exceeds the maximum allowed length.
    MsgIdTooLong = 7,
    /// Sender is sending too fast; must wait before sending again.
    RateLimitExceeded = 8,
    /// Cannot delete a message that is already deleted.
    AlreadyDeleted = 9,
    /// Caller attempted to delete another participant's message.
    CannotDeleteOthers = 10,
    /// Conversation already exists between these two participants.
    ConversationAlreadyExists = 11,
}

impl core::fmt::Display for MessagingError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::AlreadyInitialized => write!(f, "contract already initialized"),
            Self::ConversationNotFound => write!(f, "conversation not found"),
            Self::MessageNotFound => write!(f, "message not found"),
            Self::Unauthorized => write!(f, "caller is not a participant"),
            Self::MessageTooLong => write!(f, "message body exceeds maximum length"),
            Self::ConvIdTooLong => write!(f, "conversation id exceeds maximum length"),
            Self::MsgIdTooLong => write!(f, "message id exceeds maximum length"),
            Self::RateLimitExceeded => write!(f, "rate limit exceeded; wait before sending again"),
            Self::AlreadyDeleted => write!(f, "message is already deleted"),
            Self::CannotDeleteOthers => write!(f, "cannot delete another participant's message"),
            Self::ConversationAlreadyExists => write!(f, "conversation already exists"),
        }
    }
}

pub fn get_suggestion(error: MessagingError) -> Symbol {
    match error {
        MessagingError::AlreadyInitialized => symbol_short!("INIT_DUP"),
        MessagingError::ConversationNotFound => symbol_short!("NO_CONV"),
        MessagingError::MessageNotFound => symbol_short!("NO_MSG"),
        MessagingError::Unauthorized => symbol_short!("AUTH"),
        MessagingError::MessageTooLong => symbol_short!("TOO_LONG"),
        MessagingError::ConvIdTooLong => symbol_short!("ID_LONG"),
        MessagingError::MsgIdTooLong => symbol_short!("ID_LONG"),
        MessagingError::RateLimitExceeded => symbol_short!("RATE_LIM"),
        MessagingError::AlreadyDeleted => symbol_short!("DELETED"),
        MessagingError::CannotDeleteOthers => symbol_short!("NOT_OWN"),
        MessagingError::ConversationAlreadyExists => symbol_short!("CONV_DUP"),
    }
}
