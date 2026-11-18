//! Two-step admin key rotation, shared by every contract that owns an admin
//! (closes #983).
//!
//! # Why two steps
//!
//! `escrow` has shipped a careful handover for a while: the current admin
//! *proposes* a successor, and the successor must *accept* with its own
//! signature. Between the two, the current admin can cancel. That ordering is
//! what makes a mistyped or malicious transfer recoverable — a single-step
//! `transfer_admin(new_admin)` that pointed at an unreachable address would
//! lock the contract out of `pause`, `upgrade` and every config change.
//!
//! This module is that state machine, defined once. Each contract keeps owning
//! its **active** admin under its own `DataKey::Admin`; the shared key space
//! owns only the **pending** successor, so nothing an existing deployment
//! stored changes meaning.
//!
//! # One key, one meaning
//!
//! [`AdminKey::PendingAdmin`] follows the same rule `shared::guard::LockKey`
//! documents: a shared key space means one key has one meaning at every call
//! site. It is a separate `#[contracttype]` enum, so it encodes by variant name
//! and cannot collide with a contract's own `DataKey::Admin` (or with the
//! `PendingAdmin` escrow already defines locally — the two are different Rust
//! types and therefore different host keys).
//!
//! # Identity checks stay in the contract
//!
//! [`propose`] and [`cancel`] call `require_auth` on the admin they are handed,
//! but they cannot read the contract's active admin — that lives under a
//! contract-specific key. Each wrapper therefore checks `admin == stored admin`
//! *before* delegating, exactly as escrow does. Skipping that check would let
//! any signer set a pending successor and then accept it themselves.
//!
//! # No timelock
//!
//! Handover completes as soon as the successor accepts; there is deliberately
//! no delay. A timelock delays recovery when the *new* key is the one that was
//! lost or mistyped, and the threat it defends against — a malicious successor
//! racing a cancellation — does not apply, because the successor's acceptance
//! only ever installs an address the current admin already named. Escrow's
//! mechanism, and the platform's operator tooling built around it, are the
//! reference; adding a delay here would make the two diverge.

use soroban_sdk::{contracterror, contracttype, symbol_short, Address, Env, Symbol};

/// Instance-storage keys owned by the shared admin handover.
///
/// Only the pending successor lives here; the active admin is the contract's
/// own key. See the module docs for why the two are separate.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminKey {
    /// Address proposed by `transfer_admin`, awaiting `accept_admin`.
    PendingAdmin,
}

/// Typed failures of the shared admin handover (#983).
///
/// These codes are local to this state machine; each contract maps them onto
/// its own error enum (a missing contract is that contract's `NotInitialized`,
/// every other refusal its `Unauthorized`). The corresponding entries in the
/// unified catalogue (`shared::errors::SharedErrorCode`) are
/// `NoPendingAdminTransfer = 20` and `InvalidAdminTarget = 21`, which name the
/// two handover-specific conditions the catalogue did not previously have.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminError {
    /// The contract has no active admin, so a handover cannot start.
    NotInitialized = 1,
    /// The caller is not the active admin (or not the pending successor).
    Unauthorized = 2,
    /// There is no in-flight transfer to accept or cancel.
    NoPendingTransfer = 3,
    /// The proposed successor is the current admin; that is a no-op, not a
    /// rotation, and accepting it would look like a completed handover.
    InvalidTarget = 4,
}

impl core::fmt::Display for AdminError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::NotInitialized => write!(f, "contract has no active admin"),
            Self::Unauthorized => write!(f, "caller is not authorized for this admin transfer"),
            Self::NoPendingTransfer => write!(f, "no admin transfer is pending"),
            Self::InvalidTarget => write!(f, "new admin must differ from the current admin"),
        }
    }
}

pub fn get_suggestion(error: AdminError) -> Symbol {
    match error {
        AdminError::NotInitialized => symbol_short!("NO_INIT"),
        AdminError::Unauthorized => symbol_short!("AUTH"),
        AdminError::NoPendingTransfer => symbol_short!("NO_PEND"),
        AdminError::InvalidTarget => symbol_short!("SAME_ADM"),
    }
}

/// Emitted when an admin proposes a successor (#983).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminTransferProposedEvent {
    pub admin: Address,
    pub new_admin: Address,
}

/// Emitted when the proposed successor accepts and becomes the active admin.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminRotatedEvent {
    pub previous_admin: Address,
    pub new_admin: Address,
}

/// Emitted when an in-flight transfer is withdrawn without completing.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminTransferCancelledEvent {
    pub admin: Address,
}

/// The proposed successor awaiting acceptance, if any.
pub fn pending_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&AdminKey::PendingAdmin)
}

/// Step 1 — `current_admin` proposes `new_admin` as its successor.
///
/// Does not change the active admin. The caller's wrapper is responsible for
/// checking `current_admin` against its stored admin before calling this.
pub fn propose(
    env: &Env,
    current_admin: &Address,
    new_admin: &Address,
) -> Result<(), AdminError> {
    if current_admin == new_admin {
        return Err(AdminError::InvalidTarget);
    }
    current_admin.require_auth();
    env.storage()
        .instance()
        .set(&AdminKey::PendingAdmin, new_admin);
    env.events().publish(
        (symbol_short!("adm_prop"),),
        AdminTransferProposedEvent {
            admin: current_admin.clone(),
            new_admin: new_admin.clone(),
        },
    );
    Ok(())
}

/// Step 2 — the pending successor accepts and completes the rotation.
///
/// Only `new_admin`'s own signature can complete the handover; the contract
/// wrapper installs it as the active admin after this returns. `previous_admin`
/// is what the contract currently has stored, and is carried only so the
/// rotation event can name both parties.
pub fn accept(
    env: &Env,
    previous_admin: &Address,
    new_admin: &Address,
) -> Result<(), AdminError> {
    new_admin.require_auth();
    let pending: Address = env
        .storage()
        .instance()
        .get(&AdminKey::PendingAdmin)
        .ok_or(AdminError::NoPendingTransfer)?;
    if pending != *new_admin {
        return Err(AdminError::Unauthorized);
    }
    env.storage().instance().remove(&AdminKey::PendingAdmin);
    env.events().publish(
        (symbol_short!("adm_rot"),),
        AdminRotatedEvent {
            previous_admin: previous_admin.clone(),
            new_admin: new_admin.clone(),
        },
    );
    Ok(())
}

/// Withdraw an in-flight proposal without completing it; the active admin is
/// left untouched.
///
/// The contract wrapper checks the caller against its stored admin first.
pub fn cancel(env: &Env, admin: &Address) -> Result<(), AdminError> {
    admin.require_auth();
    if !env.storage().instance().has(&AdminKey::PendingAdmin) {
        return Err(AdminError::NoPendingTransfer);
    }
    env.storage().instance().remove(&AdminKey::PendingAdmin);
    env.events().publish(
        (symbol_short!("adm_cncl"),),
        AdminTransferCancelledEvent { admin: admin.clone() },
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn funded_env() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        (env, admin)
    }

    #[test]
    fn propose_then_accept_moves_the_pending_admin() {
        let (env, admin) = funded_env();
        let successor = Address::generate(&env);

        propose(&env, &admin, &successor).unwrap();
        assert_eq!(pending_admin(&env), Some(successor.clone()));

        accept(&env, &admin, &successor).unwrap();
        // The shared layer clears its pending key; the contract installs the
        // active admin, so nothing is left dangling here.
        assert_eq!(pending_admin(&env), None);
    }

    #[test]
    fn unaccepted_transfer_leaves_pending_set() {
        let (env, admin) = funded_env();
        let successor = Address::generate(&env);

        propose(&env, &admin, &successor).unwrap();
        // No accept yet: the successor is still only pending.
        assert_eq!(pending_admin(&env), Some(successor));
    }

    #[test]
    fn cancel_leaves_no_pending_and_original_admin_intact() {
        let (env, admin) = funded_env();
        let successor = Address::generate(&env);

        propose(&env, &admin, &successor).unwrap();
        cancel(&env, &admin).unwrap();
        assert_eq!(pending_admin(&env), None);

        // Cancelling twice reports there is nothing in flight.
        assert_eq!(cancel(&env, &admin), Err(AdminError::NoPendingTransfer));
    }

    #[test]
    fn transfer_to_self_is_rejected() {
        let (env, admin) = funded_env();
        assert_eq!(
            propose(&env, &admin, &admin),
            Err(AdminError::InvalidTarget)
        );
        assert_eq!(pending_admin(&env), None);
    }

    #[test]
    fn accept_before_any_transfer_is_rejected() {
        let (env, admin) = funded_env();
        let stranger = Address::generate(&env);
        assert_eq!(
            accept(&env, &admin, &stranger),
            Err(AdminError::NoPendingTransfer)
        );
    }

    #[test]
    fn accept_by_a_non_successor_is_rejected() {
        let (env, admin) = funded_env();
        let successor = Address::generate(&env);
        let impostor = Address::generate(&env);

        propose(&env, &admin, &successor).unwrap();
        assert_eq!(
            accept(&env, &admin, &impostor),
            Err(AdminError::Unauthorized)
        );
        // The impostor did not consume the proposal.
        assert_eq!(pending_admin(&env), Some(successor));
    }

    #[test]
    fn double_accept_is_rejected() {
        let (env, admin) = funded_env();
        let successor = Address::generate(&env);

        propose(&env, &admin, &successor).unwrap();
        accept(&env, &admin, &successor).unwrap();
        assert_eq!(
            accept(&env, &admin, &successor),
            Err(AdminError::NoPendingTransfer)
        );
    }

    #[test]
    fn error_codes_are_stable() {
        // Pinned: an indexer decodes the shared handover across contracts.
        assert_eq!(AdminError::NotInitialized as u32, 1);
        assert_eq!(AdminError::Unauthorized as u32, 2);
        assert_eq!(AdminError::NoPendingTransfer as u32, 3);
        assert_eq!(AdminError::InvalidTarget as u32, 4);
    }
}
