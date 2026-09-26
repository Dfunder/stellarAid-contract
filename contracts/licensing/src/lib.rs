// contracts/licensing/src/lib.rs
// Implements Creative Licensing Marketplace — closes #616
//
// Acceptance Criteria:
// - Support sub-licensing rights
// - Implement usage rights marketplace
// - Add usage tracking and auditing
// - Support license derivatives
// - Implement dispute resolution for usage

#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, String, Vec,
};

// ── Data Types ────────────────────────────────────────────────────────────────

/// The type of license being granted.
#[contracttype]
#[derive(Clone, PartialEq)]
pub enum LicenseType {
    /// Personal, non-commercial use only.
    Personal,
    /// Commercial use by the direct licensee.
    Commercial,
    /// Exclusive commercial rights — no other licenses may be issued.
    Exclusive,
    /// Sub-license derived from a parent license.
    SubLicense,
}

/// Current state of a license record.
#[contracttype]
#[derive(Clone, PartialEq)]
pub enum LicenseStatus {
    Active,
    Revoked,
    Expired,
    Disputed,
}

/// Outcome of a usage dispute.
#[contracttype]
#[derive(Clone, PartialEq)]
pub enum DisputeOutcome {
    Pending,
    UpheldForLicensor,
    UpheldForLicensee,
    Settled,
}

/// A single license record stored on-chain.
#[contracttype]
#[derive(Clone)]
pub struct LicenseRecord {
    /// Unique license identifier.
    pub id: u64,
    /// The digital asset being licensed.
    pub asset_id: String,
    /// Address of the rights owner (licensor).
    pub owner: Address,
    /// Address of the licensee.
    pub licensee: Address,
    /// License type/scope.
    pub license_type: LicenseType,
    /// Price paid (in stroops / base units).
    pub price: i128,
    /// Whether sub-licensing is permitted.
    pub sub_licensable: bool,
    /// Optional parent license id (non-zero for sub-licenses).
    pub parent_id: u64,
    /// Current license status.
    pub status: LicenseStatus,
    /// Ledger number when created.
    pub created_at: u32,
    /// Ledger number when the license expires (0 = no expiry).
    pub expires_at: u32,
}

/// A usage log entry recorded every time a licensee uses the asset.
#[contracttype]
#[derive(Clone)]
pub struct UsageEntry {
    pub license_id: u64,
    pub user: Address,
    pub context: String,
    pub ledger: u32,
}

/// A dispute raised against alleged misuse of a licensed asset.
#[contracttype]
#[derive(Clone)]
pub struct LicenseDispute {
    pub dispute_id: u64,
    pub license_id: u64,
    pub complainant: Address,
    pub description: String,
    pub outcome: DisputeOutcome,
    pub raised_at: u32,
}

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    /// Main license record.
    License(u64),
    /// Monotonic counter for license ids.
    LicenseCount,
    /// Monotonic counter for dispute ids.
    DisputeCount,
    /// A single usage entry: (license_id, index).
    Usage(u64, u64),
    /// Number of usage entries per license.
    UsageCount(u64),
    /// Dispute record.
    Dispute(u64),
    /// Admin address allowed to resolve disputes.
    Admin,
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct LicensingMarketplace;

#[contractimpl]
impl LicensingMarketplace {
    // ── Admin ─────────────────────────────────────────────────────────────

    /// Initialize the contract with an admin address.
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::LicenseCount, &0u64);
        env.storage()
            .persistent()
            .set(&DataKey::DisputeCount, &0u64);
        env.events().publish(
            (symbol_short!("init"),),
            admin,
        );
    }

    // ── License Creation ──────────────────────────────────────────────────

    /// Issue a new primary license for a digital asset.
    ///
    /// # Arguments
    /// * `owner`         — rights holder (must sign)
    /// * `licensee`      — party receiving the license
    /// * `asset_id`      — identifier of the digital work
    /// * `license_type`  — scope of rights granted
    /// * `price`         — agreed price in base units
    /// * `sub_licensable`— whether the licensee may create sub-licenses
    /// * `expires_at`    — expiry ledger; pass 0 for no expiry
    ///
    /// Returns the new license id.
    pub fn create_license(
        env: Env,
        owner: Address,
        licensee: Address,
        asset_id: String,
        license_type: LicenseType,
        price: i128,
        sub_licensable: bool,
        expires_at: u32,
    ) -> u64 {
        owner.require_auth();

        let id = Self::next_license_id(&env);
        let record = LicenseRecord {
            id,
            asset_id: asset_id.clone(),
            owner: owner.clone(),
            licensee: licensee.clone(),
            license_type: license_type.clone(),
            price,
            sub_licensable,
            parent_id: 0,
            status: LicenseStatus::Active,
            created_at: env.ledger().sequence(),
            expires_at,
        };

        env.storage()
            .persistent()
            .set(&DataKey::License(id), &record);
        env.storage()
            .persistent()
            .set(&DataKey::UsageCount(id), &0u64);

        env.events().publish(
            (symbol_short!("licensed"), asset_id),
            (id, owner, licensee, license_type),
        );

        id
    }

    /// Grant a sub-license derived from an existing license.
    ///
    /// Requires the parent license to be active and sub-licensable.
    /// The sub-licensor must be the current licensee of the parent.
    ///
    /// Returns the new sub-license id.
    pub fn create_sub_license(
        env: Env,
        parent_id: u64,
        sub_licensee: Address,
        price: i128,
        expires_at: u32,
    ) -> u64 {
        let parent: LicenseRecord = env
            .storage()
            .persistent()
            .get(&DataKey::License(parent_id))
            .expect("parent license not found");

        // Only the parent licensee may sub-license.
        parent.licensee.require_auth();

        assert!(
            parent.status == LicenseStatus::Active,
            "parent license is not active"
        );
        assert!(parent.sub_licensable, "parent license does not allow sub-licensing");

        let id = Self::next_license_id(&env);
        let record = LicenseRecord {
            id,
            asset_id: parent.asset_id.clone(),
            owner: parent.licensee.clone(),
            licensee: sub_licensee.clone(),
            license_type: LicenseType::SubLicense,
            price,
            sub_licensable: false,
            parent_id,
            status: LicenseStatus::Active,
            created_at: env.ledger().sequence(),
            expires_at,
        };

        env.storage()
            .persistent()
            .set(&DataKey::License(id), &record);
        env.storage()
            .persistent()
            .set(&DataKey::UsageCount(id), &0u64);

        env.events().publish(
            (symbol_short!("sublicens"), parent.asset_id),
            (id, parent_id, sub_licensee),
        );

        id
    }

    // ── License Management ────────────────────────────────────────────────

    /// Revoke a license.  Only the original owner may revoke.
    pub fn revoke_license(env: Env, caller: Address, license_id: u64) {
        caller.require_auth();
        let mut record: LicenseRecord = env
            .storage()
            .persistent()
            .get(&DataKey::License(license_id))
            .expect("license not found");

        assert!(
            record.owner == caller,
            "only the license owner can revoke"
        );

        record.status = LicenseStatus::Revoked;
        env.storage()
            .persistent()
            .set(&DataKey::License(license_id), &record);

        env.events()
            .publish((symbol_short!("revoked"),), license_id);
    }

    /// Mark a license as expired (admin or owner action).
    pub fn expire_license(env: Env, caller: Address, license_id: u64) {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("contract not initialized");

        let mut record: LicenseRecord = env
            .storage()
            .persistent()
            .get(&DataKey::License(license_id))
            .expect("license not found");

        assert!(
            caller == admin || caller == record.owner,
            "unauthorized"
        );

        record.status = LicenseStatus::Expired;
        env.storage()
            .persistent()
            .set(&DataKey::License(license_id), &record);

        env.events()
            .publish((symbol_short!("expired"),), license_id);
    }

    // ── Usage Tracking ────────────────────────────────────────────────────

    /// Record a usage event for audit purposes.
    /// The licensee (or sub-licensee) must sign.
    pub fn record_usage(env: Env, user: Address, license_id: u64, context: String) {
        user.require_auth();

        let record: LicenseRecord = env
            .storage()
            .persistent()
            .get(&DataKey::License(license_id))
            .expect("license not found");

        assert!(
            record.status == LicenseStatus::Active,
            "license is not active"
        );
        assert!(record.licensee == user, "caller is not the licensee");

        // Check expiry.
        let now = env.ledger().sequence();
        if record.expires_at > 0 {
            assert!(now <= record.expires_at, "license has expired");
        }

        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::UsageCount(license_id))
            .unwrap_or(0);

        let entry = UsageEntry {
            license_id,
            user: user.clone(),
            context: context.clone(),
            ledger: now,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Usage(license_id, count), &entry);
        env.storage()
            .persistent()
            .set(&DataKey::UsageCount(license_id), &(count + 1));

        env.events()
            .publish((symbol_short!("usage"), license_id), (user, context));
    }

    /// Return the number of recorded usage events for a license.
    pub fn get_usage_count(env: Env, license_id: u64) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::UsageCount(license_id))
            .unwrap_or(0)
    }

    // ── Dispute Resolution ────────────────────────────────────────────────

    /// Raise a dispute for alleged misuse of a licensed asset.
    ///
    /// Returns the dispute id.
    pub fn raise_dispute(
        env: Env,
        complainant: Address,
        license_id: u64,
        description: String,
    ) -> u64 {
        complainant.require_auth();

        let mut record: LicenseRecord = env
            .storage()
            .persistent()
            .get(&DataKey::License(license_id))
            .expect("license not found");

        // Flag the license as disputed while under review.
        record.status = LicenseStatus::Disputed;
        env.storage()
            .persistent()
            .set(&DataKey::License(license_id), &record);

        let dispute_id = Self::next_dispute_id(&env);
        let dispute = LicenseDispute {
            dispute_id,
            license_id,
            complainant: complainant.clone(),
            description: description.clone(),
            outcome: DisputeOutcome::Pending,
            raised_at: env.ledger().sequence(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::Dispute(dispute_id), &dispute);

        env.events().publish(
            (symbol_short!("dispute"), license_id),
            (dispute_id, complainant, description),
        );

        dispute_id
    }

    /// Resolve an open dispute.  Only the admin may call this.
    pub fn resolve_dispute(
        env: Env,
        admin: Address,
        dispute_id: u64,
        outcome: DisputeOutcome,
    ) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("contract not initialized");
        assert!(admin == stored_admin, "only admin can resolve disputes");

        let mut dispute: LicenseDispute = env
            .storage()
            .persistent()
            .get(&DataKey::Dispute(dispute_id))
            .expect("dispute not found");

        assert!(
            dispute.outcome == DisputeOutcome::Pending,
            "dispute already resolved"
        );

        dispute.outcome = outcome.clone();
        env.storage()
            .persistent()
            .set(&DataKey::Dispute(dispute_id), &dispute);

        // Restore or revoke the license depending on outcome.
        let mut record: LicenseRecord = env
            .storage()
            .persistent()
            .get(&DataKey::License(dispute.license_id))
            .expect("license not found");

        record.status = match outcome {
            DisputeOutcome::UpheldForLicensor => LicenseStatus::Revoked,
            _ => LicenseStatus::Active,
        };

        env.storage()
            .persistent()
            .set(&DataKey::License(dispute.license_id), &record);

        env.events()
            .publish((symbol_short!("resolved"),), (dispute_id, outcome));
    }

    // ── Queries ───────────────────────────────────────────────────────────

    /// Retrieve a license record.
    pub fn get_license(env: Env, license_id: u64) -> LicenseRecord {
        env.storage()
            .persistent()
            .get(&DataKey::License(license_id))
            .expect("license not found")
    }

    /// Retrieve a dispute record.
    pub fn get_dispute(env: Env, dispute_id: u64) -> LicenseDispute {
        env.storage()
            .persistent()
            .get(&DataKey::Dispute(dispute_id))
            .expect("dispute not found")
    }

    /// Return a paginated slice of usage entries for a license.
    pub fn get_usage_entries(
        env: Env,
        license_id: u64,
        offset: u64,
        limit: u64,
    ) -> Vec<UsageEntry> {
        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::UsageCount(license_id))
            .unwrap_or(0);

        let mut result = Vec::new(&env);
        let end = (offset + limit).min(count);
        let mut i = offset;
        while i < end {
            if let Some(entry) = env
                .storage()
                .persistent()
                .get::<DataKey, UsageEntry>(&DataKey::Usage(license_id, i))
            {
                result.push_back(entry);
            }
            i += 1;
        }
        result
    }

    // ── Internal Helpers ──────────────────────────────────────────────────

    fn next_license_id(env: &Env) -> u64 {
        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::LicenseCount)
            .unwrap_or(0);
        let next = count + 1;
        env.storage()
            .persistent()
            .set(&DataKey::LicenseCount, &next);
        next
    }

    fn next_dispute_id(env: &Env) -> u64 {
        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::DisputeCount)
            .unwrap_or(0);
        let next = count + 1;
        env.storage()
            .persistent()
            .set(&DataKey::DisputeCount, &next);
        next
    }
}
