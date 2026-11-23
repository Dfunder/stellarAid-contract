//! Input sanitization and validation helpers (closes #591).
//!
//! String parameters (titles, descriptions, memos) must be validated for
//! length before being persisted to prevent storage bloat.  This module
//! provides constant limits and validation functions used by all contracts.
//!
//! ## Limits
//! | Parameter          | Max bytes |
//! |--------------------|-----------|
//! | Title / name       | 128       |
//! | Description        | 512       |
//! | Short memo         | 256       |
//! | Commission ID      | 64        |
//! | Milestone ID       | 64        |

use soroban_sdk::{contracterror, Bytes, String};

/// Typed failures of the validation helpers, so callers branch on a
/// discriminant instead of matching panic prose.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidationError {
    /// A string parameter exceeds its maximum byte length.
    StringTooLong = 1,
    /// A `Bytes` identifier exceeds its maximum byte length.
    IdTooLong = 2,
}

// ── Maximum length constants ──────────────────────────────────────────────────

/// Maximum byte length for a title or name field.
pub const MAX_TITLE_LEN: u32 = 128;

/// Maximum byte length for a description field.
pub const MAX_DESCRIPTION_LEN: u32 = 512;

/// Maximum byte length for a short memo.
pub const MAX_MEMO_LEN: u32 = 256;

/// Maximum byte length for an identifier (commission_id, milestone_id, etc.).
pub const MAX_ID_LEN: u32 = 64;

// ── Validation helpers ────────────────────────────────────────────────────────

/// Abort with [`ValidationError::StringTooLong`] when `s` exceeds `max_len` bytes.
///
/// Soroban `String` is UTF-8 and `len()` returns the byte count.
///
/// Closes #591.
#[inline]
pub fn require_string_len(s: &String, max_len: u32, field: &str) {
    if s.len() > max_len {
        let _ = field;
        soroban_sdk::panic_with_error!(s.env(), ValidationError::StringTooLong);
    }
}

/// Returns `true` when `s` fits within `max_len` bytes, `false` otherwise.
#[inline]
pub fn is_string_len_valid(s: &String, max_len: u32) -> bool {
    s.len() <= max_len
}

/// Abort with [`ValidationError::IdTooLong`] when a `Bytes` identifier exceeds `max_len` bytes.
///
/// Closes #591.
#[inline]
pub fn require_id_len(id: &Bytes, max_len: u32, field: &str) {
    if id.len() > max_len {
        let _ = field;
        soroban_sdk::panic_with_error!(id.env(), ValidationError::IdTooLong);
    }
}

/// Returns `true` when a `Bytes` identifier fits within `max_len` bytes.
#[inline]
pub fn is_id_len_valid(id: &Bytes, max_len: u32) -> bool {
    id.len() <= max_len
}
