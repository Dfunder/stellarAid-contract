//! Verification Badge System Contract — closes #598
//!
//! Provides:
//! - [`VerificationContract::initialize`]      — one-time setup with an admin address.
//! - [`VerificationContract::request_badge`]   — artist requests a badge.
//! - [`VerificationContract::approve_badge`]   — admin approves with expiry ledger.
//! - [`VerificationContract::reject_badge`]    — admin rejects with reason.
//! - [`VerificationContract::revoke_badge`]    — admin revokes an active badge.
//! - [`VerificationContract::expire_badge`]    — anyone can expire a past-due badge.
//! - [`VerificationContract::get_badge`]       — read a badge record.
//! - [`VerificationContract::get_badge_history`] — full state history for a badge.

#![no_std]

mod errors;
mod types;

#[cfg(test)]
mod test;

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Bytes, Env, String, Vec};
use errors::VerificationError;
use types::{BadgeHistoryEntry, BadgeRecord, BadgeStatus, BadgeType, DataKey};

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn require_admin(env: &Env) -> Result<Address, VerificationError> {
    if !env.storage().instance().has(&DataKey::Admin) {
        return Err(VerificationError::NotInitialized);
    }
    let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    admin.require_auth();
    Ok(admin)
}

fn load_badge(env: &Env, badge_id: &Bytes) -> Result<BadgeRecord, VerificationError> {
    env.storage()
        .persistent()
        .get(&DataKey::Badge(badge_id.clone()))
        .ok_or(VerificationError::NotFound)
}

fn save_badge(env: &Env, badge: &BadgeRecord) {
    env.storage()
        .persistent()
        .set(&DataKey::Badge(badge.badge_id.clone()), badge);
}

fn append_history(
    env: &Env,
    badge_id: &Bytes,
    from: BadgeStatus,
    to: BadgeStatus,
    changed_by: Address,
    note: Option<String>,
) {
    let key = DataKey::BadgeHistory(badge_id.clone());
    let mut history: Vec<BadgeHistoryEntry> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(Vec::new(env));
    history.push_back(BadgeHistoryEntry {
        from_status: from,
        to_status: to,
        changed_at_ledger: env.ledger().sequence(),
        changed_by,
        note,
    });
    env.storage().persistent().set(&key, &history);
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct VerificationContract;

#[contractimpl]
impl VerificationContract {
    /// Initialise with an admin address.  Can only be called once.
    pub fn initialize(env: Env, admin: Address) -> Result<(), VerificationError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(VerificationError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.events()
            .publish((symbol_short!("ver_init"),), (admin,));
        Ok(())
    }

    /// Artist requests a verification badge.
    ///
    /// Only one pending/active badge per (artist, badge_type) is allowed.
    /// Closes #598.
    pub fn request_badge(
        env: Env,
        badge_id: Bytes,
        artist: Address,
        badge_type: BadgeType,
    ) -> Result<(), VerificationError> {
        if !env.storage().instance().has(&DataKey::Admin) {
            return Err(VerificationError::NotInitialized);
        }
        artist.require_auth();

        let idx_key = DataKey::ArtistBadgeIndex(artist.clone(), badge_type.clone());
        if env.storage().persistent().has(&idx_key) {
            return Err(VerificationError::AlreadyRequested);
        }

        let record = BadgeRecord {
            badge_id: badge_id.clone(),
            artist: artist.clone(),
            badge_type: badge_type.clone(),
            status: BadgeStatus::Pending,
            requested_ledger: env.ledger().sequence(),
            expiry_ledger: 0,
            note: None,
        };
        save_badge(&env, &record);
        env.storage()
            .persistent()
            .set(&idx_key, &badge_id.clone());

        env.events()
            .publish((symbol_short!("ver_req"),), (badge_id, artist, badge_type));
        Ok(())
    }

    /// Admin approves a pending badge request, setting its expiry.
    ///
    /// `expiry_ledger = 0` means the badge never expires.
    /// Closes #598.
    pub fn approve_badge(
        env: Env,
        badge_id: Bytes,
        expiry_ledger: u32,
    ) -> Result<(), VerificationError> {
        let admin = require_admin(&env)?;

        let mut record = load_badge(&env, &badge_id)?;
        if record.status != BadgeStatus::Pending {
            return Err(VerificationError::InvalidStatus);
        }
        if expiry_ledger > 0 && expiry_ledger <= env.ledger().sequence() {
            return Err(VerificationError::ExpiryInPast);
        }

        append_history(
            &env,
            &badge_id,
            BadgeStatus::Pending,
            BadgeStatus::Active,
            admin.clone(),
            None,
        );

        record.status = BadgeStatus::Active;
        record.expiry_ledger = expiry_ledger;
        save_badge(&env, &record);

        env.events()
            .publish((symbol_short!("ver_appr"),), (badge_id, record.artist, expiry_ledger));
        Ok(())
    }

    /// Admin rejects a pending badge request.
    ///
    /// Closes #598.
    pub fn reject_badge(
        env: Env,
        badge_id: Bytes,
        reason: String,
    ) -> Result<(), VerificationError> {
        let admin = require_admin(&env)?;

        let mut record = load_badge(&env, &badge_id)?;
        if record.status != BadgeStatus::Pending {
            return Err(VerificationError::InvalidStatus);
        }

        append_history(
            &env,
            &badge_id,
            BadgeStatus::Pending,
            BadgeStatus::Rejected,
            admin,
            Some(reason.clone()),
        );

        // Free the (artist, badge_type) index so they can re-apply.
        let idx_key = DataKey::ArtistBadgeIndex(record.artist.clone(), record.badge_type.clone());
        env.storage().persistent().remove(&idx_key);

        record.status = BadgeStatus::Rejected;
        record.note = Some(reason.clone());
        save_badge(&env, &record);

        env.events()
            .publish((symbol_short!("ver_rej"),), (badge_id, record.artist, reason));
        Ok(())
    }

    /// Admin revokes an active badge.
    ///
    /// Closes #598.
    pub fn revoke_badge(
        env: Env,
        badge_id: Bytes,
        reason: String,
    ) -> Result<(), VerificationError> {
        let admin = require_admin(&env)?;

        let mut record = load_badge(&env, &badge_id)?;
        if record.status != BadgeStatus::Active {
            return Err(VerificationError::InvalidStatus);
        }

        append_history(
            &env,
            &badge_id,
            BadgeStatus::Active,
            BadgeStatus::Revoked,
            admin,
            Some(reason.clone()),
        );

        // Free the index so the artist can re-apply after revocation.
        let idx_key = DataKey::ArtistBadgeIndex(record.artist.clone(), record.badge_type.clone());
        env.storage().persistent().remove(&idx_key);

        record.status = BadgeStatus::Revoked;
        record.note = Some(reason.clone());
        save_badge(&env, &record);

        env.events()
            .publish((symbol_short!("ver_rev"),), (badge_id, record.artist, reason));
        Ok(())
    }

    /// Transition an active badge whose `expiry_ledger` has passed to `Expired`.
    ///
    /// Anyone can trigger this permissionlessly once the ledger has passed.
    /// Closes #598.
    pub fn expire_badge(env: Env, badge_id: Bytes) -> Result<(), VerificationError> {
        if !env.storage().instance().has(&DataKey::Admin) {
            return Err(VerificationError::NotInitialized);
        }
        let mut record = load_badge(&env, &badge_id)?;

        if record.status != BadgeStatus::Active {
            return Err(VerificationError::InvalidStatus);
        }
        if record.expiry_ledger == 0 || env.ledger().sequence() < record.expiry_ledger {
            return Err(VerificationError::BadgeExpired); // not yet expired
        }

        let contract_addr = env.current_contract_address();
        append_history(
            &env,
            &badge_id,
            BadgeStatus::Active,
            BadgeStatus::Expired,
            contract_addr,
            None,
        );

        // Free index.
        let idx_key = DataKey::ArtistBadgeIndex(record.artist.clone(), record.badge_type.clone());
        env.storage().persistent().remove(&idx_key);

        record.status = BadgeStatus::Expired;
        save_badge(&env, &record);

        env.events()
            .publish((symbol_short!("ver_exp"),), (badge_id, record.artist));
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    /// Fetch a badge record.
    pub fn get_badge(env: Env, badge_id: Bytes) -> Result<BadgeRecord, VerificationError> {
        load_badge(&env, &badge_id)
    }

    /// Fetch full status history for a badge.
    pub fn get_badge_history(
        env: Env,
        badge_id: Bytes,
    ) -> Result<Vec<BadgeHistoryEntry>, VerificationError> {
        if !env
            .storage()
            .persistent()
            .has(&DataKey::Badge(badge_id.clone()))
        {
            return Err(VerificationError::NotFound);
        }
        Ok(env
            .storage()
            .persistent()
            .get(&DataKey::BadgeHistory(badge_id))
            .unwrap_or(Vec::new(&env)))
    }
}
