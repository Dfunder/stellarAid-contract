//! Licensing Marketplace Smart Contract
//!
//! Implements a creative licensing marketplace: issuing primary and
//! sub-licenses for digital assets, tracking usage for audit, and resolving
//! disputes over alleged misuse.
//!
//! Closes #616. Acceptance criteria:
//! - Support sub-licensing rights
//! - Implement usage rights marketplace
//! - Add usage tracking and auditing
//! - Support license derivatives
//! - Implement dispute resolution for usage

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, String, Vec,
};

/// Typed errors returned by the contract entry points.
///
/// These replace the `assert!`/`expect` panics that previously guarded the
/// entry points, so a caller can handle each failure programmatically.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum LicensingError {
    /// The contract has not been initialized (no admin).
    NotInitialized = 1,
    /// No license exists for the given id.
    LicenseNotFound = 2,
    /// The parent license does not exist.
    ParentNotFound = 3,
    /// The parent license is not active.
    ParentNotActive = 4,
    /// The parent license does not allow sub-licensing.
    NotSubLicensable = 5,
    /// The license is not active.
    NotActive = 6,
    /// The caller is not the licensee.
    NotLicensee = 7,
    /// The license has expired.
    Expired = 8,
    /// The caller is not permitted to perform this action.
    Unauthorized = 9,
    /// No dispute exists for the given id.
    DisputeNotFound = 10,
    /// The dispute has already been resolved.
    AlreadyResolved = 11,
}

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
    ///
    /// The admin is the only account allowed to resolve disputes. Also seeds
    /// the license and dispute id counters to zero.
    ///
    /// # Arguments
    /// * `admin` — address granted admin rights (must sign)
    ///
    /// # Panics
    /// Panics if `admin` does not sign the call.
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
    /// # Arguments
    /// * `parent_id`    — id of the parent license
    /// * `sub_licensee` — party receiving the sub-license
    /// * `price`        — agreed price in base units
    /// * `expires_at`   — expiry ledger; pass 0 for no expiry
    ///
    /// # Returns
    /// The new sub-license id.
    ///
    /// # Panics
    /// Panics if the parent license is missing, not active, not sub-licensable,
    /// or if the caller is not the parent's licensee.
    pub fn create_sub_license(
        env: Env,
        parent_id: u64,
        sub_licensee: Address,
        price: i128,
        expires_at: u32,
    ) -> Result<u64, LicensingError> {
        let parent: LicenseRecord = env
            .storage()
            .persistent()
            .get(&DataKey::License(parent_id))
            .ok_or(LicensingError::ParentNotFound)?;

        // Only the parent licensee may sub-license.
        parent.licensee.require_auth();

        if !(parent.status == LicenseStatus::Active) {
 return Err(LicensingError::ParentNotActive);
 }
        if !(parent.sub_licensable) {
 return Err(LicensingError::NotSubLicensable);
 }

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

        Ok(id)
    }

    // ── License Management ────────────────────────────────────────────────

    /// Revoke a license.  Only the original owner may revoke.
    ///
    /// Sets the license status to `Revoked`.
    ///
    /// # Arguments
    /// * `caller`     — address revoking the license (must sign)
    /// * `license_id` — id of the license to revoke
    ///
    /// # Panics
    /// Panics if the license is missing or if `caller` is not the owner.
    pub fn revoke_license(env: Env, caller: Address, license_id: u64) -> Result<(), LicensingError> {
        caller.require_auth();
        let mut record: LicenseRecord = env
            .storage()
            .persistent()
            .get(&DataKey::License(license_id))
            .ok_or(LicensingError::LicenseNotFound)?;

        if !(record.owner == caller) {
 return Err(LicensingError::Unauthorized);
 }

        record.status = LicenseStatus::Revoked;
        env.storage()
            .persistent()
            .set(&DataKey::License(license_id), &record);

        env.events()
            .publish((symbol_short!("revoked"),), license_id);
        Ok(())
    }

    /// Mark a license as expired (admin or owner action).
    ///
    /// Sets the license status to `Expired`.
    ///
    /// # Arguments
    /// * `caller`     — admin or license owner expiring the license (must sign)
    /// * `license_id` — id of the license to expire
    ///
    /// # Panics
    /// Panics if the contract is not initialized, the license is missing, or
    /// `caller` is neither the admin nor the owner.
    pub fn expire_license(env: Env, caller: Address, license_id: u64) -> Result<(), LicensingError> {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(LicensingError::NotInitialized)?;

        let mut record: LicenseRecord = env
            .storage()
            .persistent()
            .get(&DataKey::License(license_id))
            .ok_or(LicensingError::LicenseNotFound)?;

        if !(caller == admin || caller == record.owner) {
 return Err(LicensingError::Unauthorized);
 }

        record.status = LicenseStatus::Expired;
        env.storage()
            .persistent()
            .set(&DataKey::License(license_id), &record);

        env.events()
            .publish((symbol_short!("expired"),), license_id);
        Ok(())
    }

    // ── Usage Tracking ────────────────────────────────────────────────────

    /// Record a usage event for audit purposes.
    /// The licensee (or sub-licensee) must sign.
    ///
    /// Appends a `UsageEntry` to the license's usage log and emits a `usage`
    /// event.
    ///
    /// # Arguments
    /// * `user`       — the licensee recording the usage (must sign)
    /// * `license_id` — id of the license being used
    /// * `context`    — free-form description of the usage
    ///
    /// # Panics
    /// Panics if the license is missing or not active, if `user` is not the
    /// licensee, or if the license has already expired.
    pub fn record_usage(env: Env, user: Address, license_id: u64, context: String) -> Result<(), LicensingError> {
        user.require_auth();

        let record: LicenseRecord = env
            .storage()
            .persistent()
            .get(&DataKey::License(license_id))
            .ok_or(LicensingError::LicenseNotFound)?;

        if !(record.status == LicenseStatus::Active) {
 return Err(LicensingError::NotActive);
 }
        if !(record.licensee == user) {
 return Err(LicensingError::NotLicensee);
 }

        // Check expiry.
        let now = env.ledger().sequence();
        if record.expires_at > 0 {
            if !(now <= record.expires_at) {
 return Err(LicensingError::Expired);
 }
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
        Ok(())
    }

    /// Return the number of recorded usage events for a license.
    ///
    /// # Arguments
    /// * `license_id` — id of the license
    ///
    /// # Returns
    /// The number of usage entries recorded for the license (0 if none).
    pub fn get_usage_count(env: Env, license_id: u64) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::UsageCount(license_id))
            .unwrap_or(0)
    }

    // ── Dispute Resolution ────────────────────────────────────────────────

    /// Raise a dispute for alleged misuse of a licensed asset.
    ///
    /// Flags the license as `Disputed` while the dispute is under review and
    /// emits a `dispute` event.
    ///
    /// # Arguments
    /// * `complainant`  — party raising the dispute (must sign)
    /// * `license_id`   — id of the license in question
    /// * `description`  — free-form description of the alleged misuse
    ///
    /// # Returns
    /// The dispute id.
    ///
    /// # Panics
    /// Panics if the license is missing.
    pub fn raise_dispute(
        env: Env,
        complainant: Address,
        license_id: u64,
        description: String,
    ) -> Result<u64, LicensingError> {
        complainant.require_auth();

        let mut record: LicenseRecord = env
            .storage()
            .persistent()
            .get(&DataKey::License(license_id))
            .ok_or(LicensingError::LicenseNotFound)?;

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

        Ok(dispute_id)
    }

    /// Resolve an open dispute.  Only the admin may call this.
    ///
    /// Records the outcome and restores the license to `Active`, or revokes it
    /// when the dispute is upheld for the licensor.
    ///
    /// # Arguments
    /// * `admin`      — the stored admin address (must sign)
    /// * `dispute_id` — id of the dispute to resolve
    /// * `outcome`    — the resolution to record
    ///
    /// # Panics
    /// Panics if the contract is not initialized, the caller is not the admin,
    /// the dispute is missing, or the dispute was already resolved.
    pub fn resolve_dispute(
        env: Env,
        admin: Address,
        dispute_id: u64,
        outcome: DisputeOutcome,
    ) -> Result<(), LicensingError> {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(LicensingError::NotInitialized)?;
        if !(admin == stored_admin) {
 return Err(LicensingError::Unauthorized);
 }

        let mut dispute: LicenseDispute = env
            .storage()
            .persistent()
            .get(&DataKey::Dispute(dispute_id))
            .ok_or(LicensingError::DisputeNotFound)?;

        if !(dispute.outcome == DisputeOutcome::Pending) {
 return Err(LicensingError::AlreadyResolved);
 }

        dispute.outcome = outcome.clone();
        env.storage()
            .persistent()
            .set(&DataKey::Dispute(dispute_id), &dispute);

        // Restore or revoke the license depending on outcome.
        let mut record: LicenseRecord = env
            .storage()
            .persistent()
            .get(&DataKey::License(dispute.license_id))
            .ok_or(LicensingError::LicenseNotFound)?;

        record.status = match outcome {
            DisputeOutcome::UpheldForLicensor => LicenseStatus::Revoked,
            _ => LicenseStatus::Active,
        };

        env.storage()
            .persistent()
            .set(&DataKey::License(dispute.license_id), &record);

        env.events()
            .publish((symbol_short!("resolved"),), (dispute_id, outcome));
        Ok(())
    }

    // ── Queries ───────────────────────────────────────────────────────────

    /// Retrieve a license record.
    ///
    /// # Arguments
    /// * `license_id` — id of the license
    ///
    /// # Returns
    /// The stored `LicenseRecord`.
    ///
    /// # Panics
    /// Panics if the license is not found.
    pub fn get_license(env: Env, license_id: u64) -> Result<LicenseRecord, LicensingError> {
        env.storage()
            .persistent()
            .get(&DataKey::License(license_id))
            .ok_or(LicensingError::LicenseNotFound)
    }

    /// Retrieve a dispute record.
    ///
    /// # Arguments
    /// * `dispute_id` — id of the dispute
    ///
    /// # Returns
    /// The stored `LicenseDispute`.
    ///
    /// # Panics
    /// Panics if the dispute is not found.
    pub fn get_dispute(env: Env, dispute_id: u64) -> Result<LicenseDispute, LicensingError> {
        env.storage()
            .persistent()
            .get(&DataKey::Dispute(dispute_id))
            .ok_or(LicensingError::DisputeNotFound)
    }

    /// Return a paginated slice of usage entries for a license.
    ///
    /// The cursor and the limit are both clamped by `shared::pagination`, so a
    /// page spans at most `shared::pagination::MAX_LIMIT` entries however large
    /// `limit` is. A `limit` of `0` yields the default page size rather than an
    /// empty result, and an `offset` past the end of the log is an empty page
    /// rather than an error, so a client can page to the end without reading a
    /// count first.
    ///
    /// For the page metadata that makes that loop safe, use
    /// [`get_usage_entries_page`](Self::get_usage_entries_page).
    pub fn get_usage_entries(
        env: Env,
        license_id: u64,
        offset: u64,
        limit: u64,
    ) -> Vec<UsageEntry> {
        let (entries, _info) = Self::usage_entries_page(&env, license_id, offset, limit);
        entries
    }

    /// Like [`get_usage_entries`](Self::get_usage_entries), but returns the page
    /// together with its `PageInfo`: the total entry count, the next cursor, and
    /// whether anything exists past this page.
    pub fn get_usage_entries_page(
        env: Env,
        license_id: u64,
        offset: u64,
        limit: u64,
    ) -> (Vec<UsageEntry>, shared::pagination::PageInfo) {
        Self::usage_entries_page(&env, license_id, offset, limit)
    }

    // ── Internal Helpers ──────────────────────────────────────────────────

    /// Shared body of the two usage-log readers, bounded by
    /// `shared::pagination::collect_window` (closes #876).
    ///
    /// `UsageCount` and `DataKey::Usage` are keyed by `u64`, while the
    /// pagination arithmetic is `u32`. The narrowing conversions saturate
    /// rather than wrap: a log of `u32::MAX` entries is already far past
    /// anything a 100-entry page can address, and wrapping would report a
    /// small `total` for a very long log and mis-page the client.
    fn usage_entries_page(
        env: &Env,
        license_id: u64,
        offset: u64,
        limit: u64,
    ) -> (Vec<UsageEntry>, shared::pagination::PageInfo) {
        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::UsageCount(license_id))
            .unwrap_or(0);

        shared::pagination::collect_window(
            env,
            u32::try_from(count).unwrap_or(u32::MAX),
            u32::try_from(offset).unwrap_or(u32::MAX),
            u32::try_from(limit).unwrap_or(u32::MAX),
            |i| {
                env.storage()
                    .persistent()
                    .get::<DataKey, UsageEntry>(&DataKey::Usage(license_id, u64::from(i)))
            },
        )
    }

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
