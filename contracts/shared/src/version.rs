//! Contract semantic versioning (closes #682).
//!
//! Every Lumora contract exposes the same version query surface:
//!
//! | Entry point              | Purpose                                              |
//! |--------------------------|------------------------------------------------------|
//! | `get_version`            | On-chain MAJOR.MINOR.PATCH                           |
//! | `get_version_metadata`   | Name, semver, min-compatible client, storage schema  |
//! | `is_version_compatible`  | Constraint check against a required client version   |
//!
//! Version numbers are defined in each crate's `Cargo.toml` (`package.version`)
//! and persisted to instance storage at `initialize` / `record_upgrade` so
//! operators can query a live contract without knowing which WASM is installed.
//!
//! Constraint rules are documented in `docs/VERSIONING.md`.

use soroban_sdk::{contracttype, Env, String};

use crate::upgrade::{ContractVersion, UpgradeKey};

/// Storage-schema generation. Bump this when a `#[contracttype]` layout
/// change requires a migration function. Independent of the crate semver.
pub const CURRENT_STORAGE_SCHEMA: u32 = 1;

/// Full version metadata returned by `get_version_metadata`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionMetadata {
    pub name: String,
    pub version: ContractVersion,
    pub min_compatible: ContractVersion,
    pub storage_schema: u32,
}

/// Inclusive minimum / exclusive-major constraint used by clients and
/// cross-contract callers.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionConstraint {
    pub min: ContractVersion,
    /// Highest major version that still satisfies the constraint (inclusive).
    pub max_major: u32,
}

impl ContractVersion {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        ContractVersion {
            major,
            minor,
            patch,
        }
    }

    /// Semver precedence: major, then minor, then patch.
    pub fn cmp_semver(&self, other: &Self) -> core::cmp::Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
    }

    /// `true` if `self` (the running contract) can serve a caller that
    /// requires `required`.
    ///
    /// * Major must match.
    /// * On `0.x` (pre-1.0) the minor must also match; only patch may drift.
    /// * On `>= 1.0` any newer minor/patch of the same major is compatible,
    ///   and `self` must be greater than or equal to `required`.
    pub fn is_compatible_with(&self, required: &Self) -> bool {
        if self.major != required.major {
            return false;
        }
        if self.major == 0 {
            return self.minor == required.minor && self.patch >= required.patch;
        }
        self.cmp_semver(required) != core::cmp::Ordering::Less
    }

    /// `true` if this version lies inside `[min, max_major.*]`.
    pub fn satisfies(&self, constraint: &VersionConstraint) -> bool {
        self.major <= constraint.max_major && self.is_compatible_with(&constraint.min)
    }
}

/// Parse a `"MAJOR.MINOR.PATCH"` string (optional `-prerelease` suffix ignored).
pub fn parse_semver(raw: &str) -> ContractVersion {
    let mut nums = [0u32; 3];
    let mut idx = 0usize;
    let mut acc = 0u32;
    for &b in raw.as_bytes() {
        if b == b'.' {
            if idx < 3 {
                nums[idx] = acc;
                idx += 1;
                acc = 0;
            }
        } else if b.is_ascii_digit() {
            acc = acc.saturating_mul(10).saturating_add((b - b'0') as u32);
        } else {
            break;
        }
    }
    if idx < 3 {
        nums[idx] = acc;
    }
    ContractVersion::new(nums[0], nums[1], nums[2])
}

/// Lowest client version that is still compatible with `current`.
pub fn min_compatible_for(current: &ContractVersion) -> ContractVersion {
    if current.major == 0 {
        ContractVersion::new(0, current.minor, 0)
    } else {
        ContractVersion::new(current.major, 0, 0)
    }
}

/// Persist `version` under the shared upgrade key so `upgrade::get_version`
/// and `version::query` stay in sync.
pub fn store(env: &Env, version: &ContractVersion) {
    env.storage().instance().set(&UpgradeKey::Version, version);
}

/// Seed instance storage from a crate's `CARGO_PKG_VERSION`. Call from
/// `initialize`.
pub fn seed(env: &Env, pkg_version: &str) {
    store(env, &parse_semver(pkg_version));
}

/// Stored version if present, otherwise the WASM crate version.
pub fn query(env: &Env, pkg_version: &str) -> ContractVersion {
    env.storage()
        .instance()
        .get(&UpgradeKey::Version)
        .unwrap_or_else(|| parse_semver(pkg_version))
}

/// Build [`VersionMetadata`] for the calling contract.
pub fn query_metadata(env: &Env, pkg_name: &str, pkg_version: &str) -> VersionMetadata {
    let version = query(env, pkg_version);
    VersionMetadata {
        name: String::from_str(env, pkg_name),
        min_compatible: min_compatible_for(&version),
        version,
        storage_schema: CURRENT_STORAGE_SCHEMA,
    }
}

/// Install `get_version`, `get_version_metadata`, and `is_version_compatible`
/// on a `#[contractimpl]` block. Uses the calling crate's Cargo package
/// name and version so metadata stays aligned with `Cargo.toml`.
#[macro_export]
macro_rules! impl_semver_queries {
    () => {
        /// Return the contract semantic version (MAJOR.MINOR.PATCH).
        ///
        /// Reads instance storage when seeded at initialize/upgrade; otherwise
        /// falls back to this WASM's crate version from `Cargo.toml`.
        pub fn get_version(env: soroban_sdk::Env) -> $crate::upgrade::ContractVersion {
            $crate::version::query(&env, env!("CARGO_PKG_VERSION"))
        }

        /// Return crate name, semver, min-compatible client version, and
        /// storage schema number.
        pub fn get_version_metadata(env: soroban_sdk::Env) -> $crate::version::VersionMetadata {
            $crate::version::query_metadata(&env, env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
        }

        /// Return `true` if this contract can serve a client that requires
        /// `(major, minor, patch)` per `docs/VERSIONING.md`.
        pub fn is_version_compatible(
            env: soroban_sdk::Env,
            major: u32,
            minor: u32,
            patch: u32,
        ) -> bool {
            $crate::version::query(&env, env!("CARGO_PKG_VERSION")).is_compatible_with(
                &$crate::upgrade::ContractVersion {
                    major,
                    minor,
                    patch,
                },
            )
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::upgrade::ContractVersion;

    #[test]
    fn parse_semver_triplet() {
        let v = parse_semver("1.2.3");
        assert_eq!(v, ContractVersion::new(1, 2, 3));
    }

    #[test]
    fn parse_semver_ignores_prerelease() {
        let v = parse_semver("0.1.0-alpha.1");
        assert_eq!(v, ContractVersion::new(0, 1, 0));
    }

    #[test]
    fn parse_semver_partial() {
        assert_eq!(parse_semver("2"), ContractVersion::new(2, 0, 0));
        assert_eq!(parse_semver("2.4"), ContractVersion::new(2, 4, 0));
    }

    #[test]
    fn zero_x_requires_matching_minor() {
        let current = ContractVersion::new(0, 1, 4);
        assert!(current.is_compatible_with(&ContractVersion::new(0, 1, 0)));
        assert!(current.is_compatible_with(&ContractVersion::new(0, 1, 4)));
        assert!(!current.is_compatible_with(&ContractVersion::new(0, 1, 5)));
        assert!(!current.is_compatible_with(&ContractVersion::new(0, 2, 0)));
        assert!(!current.is_compatible_with(&ContractVersion::new(1, 0, 0)));
    }

    #[test]
    fn stable_major_accepts_newer_minor() {
        let current = ContractVersion::new(1, 3, 1);
        assert!(current.is_compatible_with(&ContractVersion::new(1, 0, 0)));
        assert!(current.is_compatible_with(&ContractVersion::new(1, 3, 1)));
        assert!(!current.is_compatible_with(&ContractVersion::new(1, 3, 2)));
        assert!(!current.is_compatible_with(&ContractVersion::new(1, 4, 0)));
        assert!(!current.is_compatible_with(&ContractVersion::new(2, 0, 0)));
    }

    #[test]
    fn min_compatible_pre_1() {
        assert_eq!(
            min_compatible_for(&ContractVersion::new(0, 1, 9)),
            ContractVersion::new(0, 1, 0)
        );
    }

    #[test]
    fn min_compatible_stable() {
        assert_eq!(
            min_compatible_for(&ContractVersion::new(2, 5, 3)),
            ContractVersion::new(2, 0, 0)
        );
    }

    #[test]
    fn constraint_max_major() {
        let c = VersionConstraint {
            min: ContractVersion::new(1, 0, 0),
            max_major: 1,
        };
        assert!(ContractVersion::new(1, 2, 0).satisfies(&c));
        assert!(!ContractVersion::new(2, 0, 0).satisfies(&c));
        assert!(!ContractVersion::new(0, 9, 0).satisfies(&c));
    }

    #[test]
    fn cmp_semver_order() {
        use core::cmp::Ordering;
        assert_eq!(
            ContractVersion::new(1, 0, 0).cmp_semver(&ContractVersion::new(1, 0, 1)),
            Ordering::Less
        );
        assert_eq!(
            ContractVersion::new(2, 0, 0).cmp_semver(&ContractVersion::new(1, 9, 9)),
            Ordering::Greater
        );
        assert_eq!(
            ContractVersion::new(1, 2, 3).cmp_semver(&ContractVersion::new(1, 2, 3)),
            Ordering::Equal
        );
    }
}
