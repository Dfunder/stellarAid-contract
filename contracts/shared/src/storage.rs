//! Typed storage helpers.
//!
//! Production code used to spell out every storage access by hand:
//!
//! ```ignore
//! env.storage()
//!     .persistent()
//!     .get(&DataKey::Record(id))
//!     .ok_or(Error::NotFound)
//! ```
//!
//! That pattern was repeated hundreds of times across the workspace. Each copy
//! is a chance to pick the wrong tier (`instance` vs `persistent`), forget a
//! default, or `unwrap()` a missing key into an opaque host panic. These
//! helpers name the intent once:
//!
//! ```ignore
//! use shared::storage::{persistent, instance};
//!
//! let record: Record = persistent::require(&env, &DataKey::Record(id), Error::NotFound)?;
//! persistent::set(&env, &DataKey::Record(id), &record);
//! let fee: u32 = instance::get_or(&env, &DataKey::FeeBps, 0);
//! ```
//!
//! They are zero-cost wrappers over `soroban_sdk::storage`: the same host
//! calls, the same key and value encoding, so migrating a call site changes
//! no storage layout. New code should use them; existing sites can migrate
//! file by file.

use soroban_sdk::{Env, IntoVal, TryFromVal, Val};

macro_rules! tier {
    ($name:ident, $accessor:ident, $doc:literal) => {
        #[doc = $doc]
        pub mod $name {
            use super::*;

            /// Read `key`, returning `None` when it is absent.
            #[inline]
            pub fn get<K, V>(env: &Env, key: &K) -> Option<V>
            where
                K: IntoVal<Env, Val>,
                V: TryFromVal<Env, Val>,
            {
                env.storage().$accessor().get(key)
            }

            /// Read `key`, falling back to `default` when it is absent.
            #[inline]
            pub fn get_or<K, V>(env: &Env, key: &K, default: V) -> V
            where
                K: IntoVal<Env, Val>,
                V: TryFromVal<Env, Val>,
            {
                get(env, key).unwrap_or(default)
            }

            /// Read `key`, computing a fallback only when it is absent.
            #[inline]
            pub fn get_or_else<K, V, F>(env: &Env, key: &K, default: F) -> V
            where
                K: IntoVal<Env, Val>,
                V: TryFromVal<Env, Val>,
                F: FnOnce() -> V,
            {
                get(env, key).unwrap_or_else(default)
            }

            /// Read `key`, or return `err` when it is absent.
            ///
            /// Use this instead of `.get(..).unwrap()`, so a missing record
            /// surfaces as a typed contract error rather than a host panic.
            #[inline]
            pub fn require<K, V, E>(env: &Env, key: &K, err: E) -> Result<V, E>
            where
                K: IntoVal<Env, Val>,
                V: TryFromVal<Env, Val>,
            {
                get(env, key).ok_or(err)
            }

            /// Write `value` under `key`.
            #[inline]
            pub fn set<K, V>(env: &Env, key: &K, value: &V)
            where
                K: IntoVal<Env, Val>,
                V: IntoVal<Env, Val>,
            {
                env.storage().$accessor().set(key, value)
            }

            /// `true` when `key` is present.
            #[inline]
            pub fn has<K>(env: &Env, key: &K) -> bool
            where
                K: IntoVal<Env, Val>,
            {
                env.storage().$accessor().has(key)
            }

            /// Delete `key` (a no-op when absent).
            #[inline]
            pub fn remove<K>(env: &Env, key: &K)
            where
                K: IntoVal<Env, Val>,
            {
                env.storage().$accessor().remove(key)
            }
        }
    };
}

tier!(
    instance,
    instance,
    "Instance storage: contract-wide config and counters that share the contract's TTL."
);
tier!(
    persistent,
    persistent,
    "Persistent storage: per-entity records with their own TTL."
);
tier!(
    temporary,
    temporary,
    "Temporary storage: short-lived entries that may be evicted."
);

#[cfg(test)]
mod tests {
    use super::{instance, persistent};
    use soroban_sdk::{contract, symbol_short, Env};

    #[contract]
    struct Host;

    #[test]
    fn round_trip_and_defaults() {
        let env = Env::default();
        let id = env.register_contract(None, Host);
        env.as_contract(&id, || {
            let k = symbol_short!("k");
            assert_eq!(instance::get::<_, u32>(&env, &k), None);
            assert_eq!(instance::get_or(&env, &k, 7_u32), 7);
            assert_eq!(persistent::require::<_, u32, _>(&env, &k, "missing"), Err("missing"));

            instance::set(&env, &k, &42_u32);
            assert!(instance::has(&env, &k));
            assert_eq!(instance::get_or(&env, &k, 7_u32), 42);
            // Tiers are independent.
            assert!(!persistent::has(&env, &k));

            persistent::set(&env, &k, &5_u32);
            assert_eq!(persistent::require::<_, u32, ()>(&env, &k, ()), Ok(5));
            persistent::remove(&env, &k);
            assert!(!persistent::has(&env, &k));
        });
    }
}
