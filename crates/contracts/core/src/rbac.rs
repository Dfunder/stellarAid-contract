//! Role-Based Access Control (RBAC) System
//!
//! Provides a secure and unified way to manage administrative roles and verify permissions
//! using Soroban's native `require_auth()` mechanism.

use soroban_sdk::{contracttype, Address, Env, Vec};

/// Storage keys for RBAC
#[contracttype]
pub enum RbacStorageKey {
    /// The global administrator address
    CoreAdmin,
    /// Addresses allowed to approve multi-signature withdrawals
    WithdrawalApprovers,
}

/// Helper functions for managing account roles
pub struct Rbac;

impl Rbac {
    /// Get the current administrator address from storage
    pub fn get_admin(env: &Env) -> Option<Address> {
        env.storage().instance().get(&RbacStorageKey::CoreAdmin)
    }

    /// Set a new administrator address (used during initialization)
    pub fn set_admin(env: &Env, admin: &Address) {
        env.storage().instance().set(&RbacStorageKey::CoreAdmin, admin);
    }

    /// Initialize withdrawal approvers with the admin address.
    pub fn init_approvers(env: &Env, admin: &Address) {
        if env.storage().instance().has(&RbacStorageKey::WithdrawalApprovers) {
            return;
        }

        let mut approvers = Vec::new(env);
        approvers.push_back(admin.clone());
        env.storage()
            .instance()
            .set(&RbacStorageKey::WithdrawalApprovers, &approvers);
    }

    /// Get all configured withdrawal approvers.
    pub fn get_approvers(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&RbacStorageKey::WithdrawalApprovers)
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Check whether an address is configured as a withdrawal approver.
    pub fn is_approver(env: &Env, address: &Address) -> bool {
        Self::get_approvers(env).contains(address)
    }

    /// Check if an administrator is set
    pub fn has_admin(env: &Env) -> bool {
        env.storage().instance().has(&RbacStorageKey::CoreAdmin)
    }

    /// Verify that the stored admin has authorized the current operation.
    /// Panics if the admin is not set or authorization fails.
    pub fn require_admin(env: &Env) {
        if let Some(admin) = Self::get_admin(env) {
            admin.require_auth();
        } else {
            panic!("Admin not initialized");
        }
    }

    /// Verify that a specific address is the admin and has authorized the operation.
    pub fn require_admin_auth(env: &Env, caller: &Address) {
        if let Some(admin) = Self::get_admin(env) {
            if caller == &admin {
                caller.require_auth();
            } else {
                panic!("Unauthorized: caller is not admin");
            }
        } else {
            panic!("Admin not initialized");
        }
    }

    /// Require that caller is a configured withdrawal approver and has signed.
    pub fn require_approver_auth(env: &Env, caller: &Address) {
        if !Self::is_approver(env, caller) {
            panic!("Unauthorized: caller is not an approver");
        }

        caller.require_auth();
    }

    /// Add a new withdrawal approver (admin only).
    pub fn add_approver(env: &Env, caller: &Address, approver: &Address) {
        Self::require_admin_auth(env, caller);

        let mut approvers = Self::get_approvers(env);
        if approvers.contains(approver) {
            panic!("Approver already exists");
        }

        approvers.push_back(approver.clone());
        env.storage()
            .instance()
            .set(&RbacStorageKey::WithdrawalApprovers, &approvers);
    }

    /// Remove an existing withdrawal approver (admin only).
    pub fn remove_approver(env: &Env, caller: &Address, approver: &Address) {
        Self::require_admin_auth(env, caller);

        let approvers = Self::get_approvers(env);
        if !approvers.contains(approver) {
            panic!("Approver does not exist");
        }

        if approvers.len() <= 1 {
            panic!("At least one approver must remain configured");
        }

        let mut filtered = Vec::new(env);
        for entry in approvers.iter() {
            if entry != approver.clone() {
                filtered.push_back(entry);
            }
        }

        env.storage()
            .instance()
            .set(&RbacStorageKey::WithdrawalApprovers, &filtered);
    }

    /// Update the administrator address (admin only)
    pub fn update_admin(env: &Env, caller: &Address, new_admin: &Address) {
        Self::require_admin_auth(env, caller);
        Self::set_admin(env, new_admin);
    }
}
