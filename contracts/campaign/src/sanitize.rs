//! Input sanitization for string and hash fields (issue #765).
//!
//! # The three rules
//!
//! 1. **Free-form text is hashed, not stored.** Anything that is prose — a
//!    dispute reason, a cancellation reason, a review note — is persisted as a
//!    `BytesN<32>` digest. On-chain storage is priced per byte and is permanent,
//!    so an unbounded `String` in a record is a permanent liability for whoever
//!    wrote it.
//! 2. **Asset codes are at most 12 characters.** That is the Stellar convention
//!    (`stellar.toml` asset codes, SEP-41 symbols) and it is enforced rather
//!    than assumed, so a malformed code cannot reach an indexer or a wallet.
//! 3. **No unbounded `String` in persistent storage.** Anything that must stay
//!    readable on chain — a memo, a resolution note — is bounded by an explicit
//!    limit from `shared::validation` before it is written.
//!
//! # Where the limits live
//!
//! The byte limits are constants in `shared::validation` so every contract
//! shares one number for "a title" or "a memo". This module re-exports the ones
//! the campaign contract enforces and adds the hash and asset-code rules, which
//! are campaign-specific.
//!
//! # Panic style
//!
//! String panics, matching the convention already used by this contract.
//! `InvalidAssets` is the typed equivalent of
//! `ASSET_CODE_TOO_LONG` / `ASSET_CODE_EMPTY` in
//! `contracts/campaign/src/errors.rs`.

// The re-exported limits and the read-only predicates are a shared vocabulary
// for callers across the contract family; this crate is a `cdylib`, so rustc
// would otherwise report the ones no entry point references yet.
#![allow(dead_code)]

use soroban_sdk::{BytesN, Env, String};

pub use shared::validation::{
    MAX_DESCRIPTION_LEN, MAX_ID_LEN, MAX_MEMO_LEN, MAX_TITLE_LEN,
};

/// Maximum length of an asset code, in bytes. The Stellar convention.
pub const MAX_ASSET_CODE_LEN: u32 = 12;

/// Error string for an asset code that is empty or longer than
/// [`MAX_ASSET_CODE_LEN`]. The typed equivalent is `Error::InvalidAssets`.
pub const INVALID_ASSETS: &str = "InvalidAssets";

/// Error string for a zero-filled hash, which carries no information and is
/// almost always an uninitialised placeholder.
pub const EMPTY_HASH: &str = "InvalidAssets: reason hash must not be the zero hash";

/// Rejects an asset code that is empty or over-length, and returns it.
///
/// Note the length check is on **bytes**, not characters: a multi-byte
/// character can be several bytes, and the on-chain limit is a byte budget.
pub fn require_asset_code(_env: &Env, asset_code: &String) -> String {
    if asset_code.is_empty() || asset_code.len() > MAX_ASSET_CODE_LEN {
        panic!("{}", INVALID_ASSETS);
    }
    asset_code.clone()
}

/// Rejects a hash that is all zeroes. A zero `BytesN<32>` is what an
/// uninitialised buffer looks like, so accepting it would let a caller satisfy a
/// "reason provided" check without providing a reason.
pub fn require_reason_hash(env: &Env, reason_hash: &BytesN<32>) -> BytesN<32> {
    if *reason_hash == BytesN::from_array(env, &[0u8; 32]) {
        panic!("{}", EMPTY_HASH);
    }
    reason_hash.clone()
}

/// Rejects free-form text that is over-length. Use for the rare field that has
/// to stay readable on chain (a memo, a resolution note); prefer
/// [`require_reason_hash`] wherever the text can live off chain.
pub fn require_bounded(_env: &Env, value: &String, max_len: u32, field: &str) -> String {
    if value.len() > max_len {
        panic!("{} exceeds maximum length of {} bytes", field, max_len);
    }
    value.clone()
}

/// True when `asset_code` would be accepted. Read-only counterpart of
/// [`require_asset_code`], for callers that want to branch instead of abort.
pub fn is_valid_asset_code(asset_code: &String) -> bool {
    !asset_code.is_empty() && asset_code.len() <= MAX_ASSET_CODE_LEN
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn asset_code_accepts_the_stellar_maximum() {
        let env = Env::default();
        let ok = String::from_str(&env, "ABCDEFGHIJKL");
        assert_eq!(ok.len(), MAX_ASSET_CODE_LEN);
        assert!(is_valid_asset_code(&ok));
        assert_eq!(require_asset_code(&env, &ok), ok);
    }

    #[test]
    fn asset_code_rejects_empty() {
        let env = Env::default();
        let empty = String::from_str(&env, "");
        assert!(!is_valid_asset_code(&empty));
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            require_asset_code(&env, &empty)
        }));
        assert!(res.is_err());
    }

    #[test]
    fn asset_code_rejects_one_character_too_many() {
        let env = Env::default();
        let too_long = String::from_str(&env, "ABCDEFGHIJKLM");
        assert!(!is_valid_asset_code(&too_long));
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            require_asset_code(&env, &too_long)
        }));
        assert!(res.is_err());
    }

    #[test]
    fn asset_code_length_is_measured_in_bytes_not_characters() {
        let env = Env::default();
        // Twelve multi-byte characters is well over the twelve-*byte* budget.
        let multi_byte = String::from_str(&env, "££££££££££££");
        assert!(multi_byte.len() > MAX_ASSET_CODE_LEN);
        assert!(!is_valid_asset_code(&multi_byte));
    }

    #[test]
    fn zero_hash_is_rejected_and_a_real_hash_passes() {
        let env = Env::default();
        let zero = BytesN::from_array(&env, &[0u8; 32]);
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            require_reason_hash(&env, &zero)
        }));
        assert!(res.is_err());

        let real = BytesN::from_array(&env, &[7u8; 32]);
        assert_eq!(require_reason_hash(&env, &real), real);
    }

    #[test]
    fn bounded_text_rejects_over_length_input() {
        let env = Env::default();
        let long = String::from_str(&env, "0123456789012345678901234567890123456789");
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            require_bounded(&env, &long, 32, "memo")
        }));
        assert!(res.is_err());

        let short = String::from_str(&env, "happy birthday");
        assert_eq!(require_bounded(&env, &short, 32, "memo"), short);
    }
}
