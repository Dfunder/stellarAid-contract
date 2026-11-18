//! Shared initialization guard (`require_initialized`).
//!
//! Every contract that gates its entry points on having been initialized asks
//! the same question — "has the admin key been written to instance storage?" —
//! and the only thing that differs between them is the contract's own error
//! type. This module centralises that check so the lookup is written once and a
//! future change to where the marker lives is made in a single place.
//!
//! The caller supplies both the storage key and the error to return, so each
//! contract keeps its own typed `NotInitialized` variant while sharing the
//! lookup and the branch.

use soroban_sdk::{Env, IntoVal, Val};

/// Returns `Ok(())` if instance storage holds `admin_key`, otherwise `err`.
///
/// The key is generic so a contract can pass its own `DataKey` variant, and the
/// error is generic so the contract keeps its own `Error::NotInitialized`
/// variant.
///
/// ```ignore
/// shared::initialization::require_initialized(&env, &DataKey::Admin, FundError::NotInitialized)?;
/// ```
pub fn require_initialized<K, E>(env: &Env, admin_key: &K, err: E) -> Result<(), E>
where
    K: IntoVal<Env, Val>,
{
    if env.storage().instance().has(admin_key) {
        Ok(())
    } else {
        Err(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    enum TestError {
        NotInitialized,
    }

    #[test]
    fn ok_when_the_key_is_present() {
        let env = Env::default();
        env.storage().instance().set(&7u32, &true);
        let result: Result<(), TestError> =
            require_initialized(&env, &7u32, TestError::NotInitialized);
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn err_when_the_key_is_absent() {
        let env = Env::default();
        let result: Result<(), TestError> =
            require_initialized(&env, &7u32, TestError::NotInitialized);
        assert_eq!(result, Err(TestError::NotInitialized));
    }
}
