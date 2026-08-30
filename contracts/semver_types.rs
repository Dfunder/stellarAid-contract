// Compile-time semantic version helpers included by contracts that do not
// depend on `shared`. Keep in lockstep with `shared::version` (closes #682).
//
// NOTE: this file is brought in via `include!("../../semver_types.rs")` from
// an arbitrary position inside each contract's `lib.rs` (typically after some
// `use` statements), so its module-level doc comment must use regular `//`
// comments rather than inner doc comments (`//!`) — `//!` is only legal as
// the very first item in a file/module, which does not hold once it is
// textually spliced in mid-file.

#[soroban_sdk::contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContractVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

#[soroban_sdk::contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionMetadata {
    pub name: soroban_sdk::String,
    pub version: ContractVersion,
    pub min_compatible: ContractVersion,
    pub storage_schema: u32,
}

pub(crate) const CURRENT_STORAGE_SCHEMA: u32 = 1;

pub(crate) fn parse_pkg_semver(raw: &str) -> ContractVersion {
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
    ContractVersion {
        major: nums[0],
        minor: nums[1],
        patch: nums[2],
    }
}

pub(crate) fn min_compatible_for(current: &ContractVersion) -> ContractVersion {
    if current.major == 0 {
        ContractVersion {
            major: 0,
            minor: current.minor,
            patch: 0,
        }
    } else {
        ContractVersion {
            major: current.major,
            minor: 0,
            patch: 0,
        }
    }
}

pub(crate) fn is_compatible(current: &ContractVersion, required: &ContractVersion) -> bool {
    if current.major != required.major {
        return false;
    }
    if current.major == 0 {
        return current.minor == required.minor && current.patch >= required.patch;
    }
    (current.minor, current.patch) >= (required.minor, required.patch)
}

/// Install `get_version`, `get_version_metadata`, and `is_version_compatible`
/// on a `#[contractimpl]` block. Uses this crate's Cargo.toml version.
#[allow(unused_macros)]
macro_rules! impl_semver_queries {
    () => {
        /// Return the contract semantic version (MAJOR.MINOR.PATCH) from Cargo.toml.
        pub fn get_version(_env: soroban_sdk::Env) -> ContractVersion {
            parse_pkg_semver(env!("CARGO_PKG_VERSION"))
        }

        /// Return crate name, semver, min-compatible client version, and storage schema.
        pub fn get_version_metadata(env: soroban_sdk::Env) -> VersionMetadata {
            let version = parse_pkg_semver(env!("CARGO_PKG_VERSION"));
            VersionMetadata {
                name: soroban_sdk::String::from_str(&env, env!("CARGO_PKG_NAME")),
                min_compatible: min_compatible_for(&version),
                version,
                storage_schema: CURRENT_STORAGE_SCHEMA,
            }
        }

        /// Return `true` if this WASM can serve a client that requires `(major, minor, patch)`.
        pub fn is_version_compatible(
            _env: soroban_sdk::Env,
            major: u32,
            minor: u32,
            patch: u32,
        ) -> bool {
            is_compatible(
                &parse_pkg_semver(env!("CARGO_PKG_VERSION")),
                &ContractVersion {
                    major,
                    minor,
                    patch,
                },
            )
        }
    };
}
