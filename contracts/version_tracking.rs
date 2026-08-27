// contracts/version_tracking.rs
// Off-chain version registry helper (closes #576).
//
// On-chain semantic versioning lives in `shared::version` and is queried via
// `get_version` / `get_version_metadata` on each contract (closes #682).
// Keep the two in lockstep: Cargo.toml `package.version` is the source of truth.

pub struct ContractVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl ContractVersion {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        ContractVersion { major, minor, patch }
    }

    pub fn to_string(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }

    pub fn increment_patch(&mut self) {
        self.patch += 1;
    }

    pub fn increment_minor(&mut self) {
        self.minor += 1;
        self.patch = 0;
    }

    pub fn increment_major(&mut self) {
        self.major += 1;
        self.minor = 0;
        self.patch = 0;
    }
}

pub trait Versioned {
    fn get_version(&self) -> &ContractVersion;
    fn version_string(&self) -> String {
        self.get_version().to_string()
    }
}

pub struct VersionRegistry {
    pub contract_name: String,
    pub version: ContractVersion,
}

impl VersionRegistry {
    pub fn new(name: &str, major: u32, minor: u32, patch: u32) -> Self {
        VersionRegistry {
            contract_name: name.to_string(),
            version: ContractVersion::new(major, minor, patch),
        }
    }
}