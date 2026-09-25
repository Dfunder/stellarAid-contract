//! Multi-Signature Authorization Contract
//!
//! Closes #709 – Add Multi-Signature Authorization Support.
//!
//! ## What this contract does
//!
//! It owns a signer set, a threshold, and a signature lifetime, and runs the
//! authorisation state machine over them:
//!
//! 1. `configure` stores the signer set, the
//!    threshold (`2-of-3` and `3-of-5` are the two shapes the issue calls out,
//!    but nothing is specialised to them) and the signature-expiration timeout.
//! 2. `open_proposal` puts an opaque action
//!    payload up for signature.
//! 3. `approve` collects one signature from one
//!    authorised signer, rejecting duplicates.
//! 4. `finalize` checks the threshold, or fails
//!    closed on an incomplete or expired set.
//!
//! ## Signature validation, honestly
//!
//! In Soroban a contract `Address` **is** a Stellar account public key, and
//! `Address::require_auth` asks the host to verify that account's signature
//! over the invocation. That is the mechanism this contract uses, and it is
//! the idiomatic one — so `approve` calls `signer.require_auth()` and the host
//! proves the signature. Three consequences are load-bearing:
//!
//! * There is deliberately **no** "submit a signature on behalf of someone
//!   else" path. `approve` takes the signer as a parameter and immediately
//!   demands that signer's auth, so an approval can only ever be recorded for
//!   the account that actually signed.
//! * A signature is bound to the proposal, and therefore to the action payload
//!   that proposal carries. Approving proposal A does not carry over to
//!   proposal B, and the same signer's approval of A cannot be replayed against
//!   a different action.
//! * No private key or opaque signature blob is ever stored — the contract
//!   records only *that* an account approved, keyed by `(proposal, signer)`, so
//!   nothing transferable is kept on-chain.
//!
//! This repository has no precedent for on-chain cryptographic verification
//! inside a contract (`ed25519-dalek` appears only in the off-chain `sdk`
//! crate), so none is invented here. A stronger scheme — e.g. verifying an
//! arbitrary off-chain signature blob so several signers can sign in one
//! transaction — is a **maintainer decision**: it needs a signature encoding, a
//! recovery scheme, and a domain-separation decision that should not be
//! guessed at in a PR that closes an authorisation issue.
//!
//! ## Fail-closed posture
//!
//! * Signature sets expire by ledger sequence; an expired proposal is marked
//!   [`ProposalStatus::Expired`] and can never be approved.
//! * `finalize` refuses when the threshold has not been reached, leaving the
//!   proposal `Pending` rather than promoting it.
//! * A signature cannot be withdrawn. That is deliberate: it is what makes
//!   "N of M collected" mean N distinct signers.
//! * A signer set that could never meet its own threshold — fewer signers than
//!   the threshold, a threshold below 2, an empty or duplicated set — is
//!   rejected at configuration time, not discovered at use time.

#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Bytes, Env, Vec};

pub mod errors;
pub mod types;

#[cfg(test)]
mod test;

use errors::MultiSigError;
use types::{
    DataKey, MultiSigConfig, Proposal, ProposalStatus, MAX_EXPIRY_LEDGERS, MAX_SIGNERS, MIN_THRESHOLD,
};

include!("../../semver_types.rs");

/// Retention for collected signatures and their proposal markers: ~90 days at
/// 6 s/ledger, matching `contracts/analytics`. A proposal that is neither
/// finalised nor expired is dead well before this.
const MULTISIG_TTL_LEDGERS: u32 = 1_296_000;

#[contract]
pub struct MultiSigContract;

fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

/// Load the configuration, or report that `configure` has not run.
fn load_config(env: &Env) -> Result<MultiSigConfig, MultiSigError> {
    env.storage()
        .instance()
        .get(&DataKey::Config)
        .ok_or(MultiSigError::NotInitialized)
}

/// Reject a signer set that could never satisfy its own threshold.
///
/// This runs at configuration time so a misconfiguration is a rejected
/// transaction rather than a contract that is permanently unable to authorise
/// anything.
fn validate_config(signers: &Vec<Address>, threshold: u32, expiry_ledgers: u32) -> Result<(), MultiSigError> {
    if signers.is_empty() || signers.len() > MAX_SIGNERS {
        return Err(MultiSigError::InvalidSigners);
    }
    // O(n^2) over at most MAX_SIGNERS entries: a duplicated address in the
    // signer set would let one account satisfy a 2-of-3 on its own.
    for (i, signer) in signers.iter().enumerate() {
        for other in signers.iter().skip(i + 1) {
            if other == signer {
                return Err(MultiSigError::InvalidSigners);
            }
        }
    }
    if threshold < MIN_THRESHOLD || threshold > signers.len() {
        return Err(MultiSigError::InvalidThreshold);
    }
    if expiry_ledgers == 0 || expiry_ledgers > MAX_EXPIRY_LEDGERS {
        return Err(MultiSigError::InvalidExpiry);
    }
    Ok(())
}

fn is_signer_in(config: &MultiSigConfig, address: &Address) -> bool {
    for signer in config.signers.iter() {
        if &signer == address {
            return true;
        }
    }
    false
}

fn load_proposal(env: &Env, id: &Bytes) -> Result<Proposal, MultiSigError> {
    env.storage()
        .persistent()
        .get(&DataKey::Proposal(id.clone()))
        .ok_or(MultiSigError::ProposalNotFound)
}

fn save_proposal(env: &Env, proposal: &Proposal) {
    let key = DataKey::Proposal(proposal.id.clone());
    env.storage().persistent().set(&key, proposal);
    env.storage()
        .persistent()
        .extend_ttl(&key, MULTISIG_TTL_LEDGERS, MULTISIG_TTL_LEDGERS);
}

fn approval_key(id: &Bytes, signer: &Address) -> DataKey {
    DataKey::Approval(id.clone(), signer.clone())
}

#[contractimpl]
impl MultiSigContract {
    /// One-shot bootstrap. The admin is the only address that may rewrite the
    /// signer set afterwards.
    pub fn initialize(env: Env, admin: Address) -> Result<(), MultiSigError> {
        admin.require_auth();
        if has_admin(&env) {
            return Err(MultiSigError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.events().publish((symbol_short!("ms_init"),), admin);
        Ok(())
    }

    impl_semver_queries!();

    /// Replace the signer set, threshold, and signature lifetime.
    ///
    /// The whole set is replaced rather than edited, so there is never a moment
    /// where the stored threshold and the stored signers disagree.
    pub fn configure(
        env: Env,
        admin: Address,
        signers: Vec<Address>,
        threshold: u32,
        expiry_ledgers: u32,
    ) -> Result<(), MultiSigError> {
        let stored: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(MultiSigError::NotInitialized)?;
        admin.require_auth();
        if stored != admin {
            return Err(MultiSigError::Unauthorized);
        }
        validate_config(&signers, threshold, expiry_ledgers)?;

        let config = MultiSigConfig {
            signers: signers.clone(),
            threshold,
            expiry_ledgers,
        };
        env.storage().instance().set(&DataKey::Config, &config);
        env.events().publish(
            (symbol_short!("ms_cfg"),),
            (signers.len(), threshold, expiry_ledgers),
        );
        Ok(())
    }

    /// Open a proposal for `action`.
    ///
    /// The expiry comes from the configuration's `expiry_ledgers`; a proposal
    /// cannot be opened with a longer life than the signers agreed to.
    pub fn open_proposal(
        env: Env,
        creator: Address,
        proposal_id: Bytes,
        action: Bytes,
    ) -> Result<Proposal, MultiSigError> {
        creator.require_auth();
        let config = load_config(&env)?;
        if action.len() == 0 {
            return Err(MultiSigError::InvalidAction);
        }
        if env
            .storage()
            .persistent()
            .has(&DataKey::Proposal(proposal_id.clone()))
        {
            return Err(MultiSigError::ProposalExists);
        }

        let created_ledger = env.ledger().sequence();
        let proposal = Proposal {
            id: proposal_id.clone(),
            action,
            creator: creator.clone(),
            status: ProposalStatus::Pending,
            signature_count: 0,
            created_ledger,
            expires_ledger: created_ledger
                .checked_add(config.expiry_ledgers)
                .ok_or(MultiSigError::InvalidExpiry)?,
        };
        save_proposal(&env, &proposal);

        let count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ProposalCount)
            .unwrap_or(0u32);
        env.storage().instance().set(&DataKey::ProposalCount, &(count + 1));

        env.events().publish(
            (symbol_short!("ms_prop"),),
            (proposal_id, creator, proposal.expires_ledger),
        );
        Ok(proposal)
    }

    /// Collect one signature for a proposal.
    ///
    /// `signer.require_auth()` is the signature check: the host verifies that
    /// account's signature over this very invocation, so a signature recorded
    /// here is an authorisation of this proposal's `action` by that account and
    /// nothing else. A second approval from the same signer is rejected rather
    /// than counted twice.
    pub fn approve(
        env: Env,
        proposal_id: Bytes,
        signer: Address,
    ) -> Result<Proposal, MultiSigError> {
        signer.require_auth();
        let config = load_config(&env)?;
        if !is_signer_in(&config, &signer) {
            return Err(MultiSigError::NotASigner);
        }
        let mut proposal = load_proposal(&env, &proposal_id)?;
        if proposal.status != ProposalStatus::Pending {
            return Err(MultiSigError::ProposalClosed);
        }
        let now = env.ledger().sequence();
        if now >= proposal.expires_ledger {
            return Err(MultiSigError::ProposalExpired);
        }
        let key = approval_key(&proposal_id, &signer);
        if env.storage().persistent().has(&key) {
            return Err(MultiSigError::DuplicateSigner);
        }

        env.storage().persistent().set(&key, &now);
        env.storage().persistent().extend_ttl(
            &key,
            MULTISIG_TTL_LEDGERS,
            MULTISIG_TTL_LEDGERS,
        );
        proposal.signature_count = proposal
            .signature_count
            .checked_add(1)
            .ok_or(MultiSigError::ThresholdNotMet)?;
        save_proposal(&env, &proposal);

        env.events().publish(
            (symbol_short!("ms_appr"),),
            (proposal_id, signer, proposal.signature_count, config.threshold),
        );
        Ok(proposal)
    }

    /// Close a proposal: `Approved` once the threshold is reached, otherwise a
    /// typed failure that leaves the proposal untouched.
    ///
    /// An elapsed signature window marks the proposal `Expired` and is
    /// terminal — an expired set is never retroactively approved.
    pub fn finalize(
        env: Env,
        proposal_id: Bytes,
        executor: Address,
    ) -> Result<ProposalStatus, MultiSigError> {
        executor.require_auth();
        let config = load_config(&env)?;
        let mut proposal = load_proposal(&env, &proposal_id)?;
        if proposal.status != ProposalStatus::Pending {
            return Err(MultiSigError::ProposalClosed);
        }
        if env.ledger().sequence() >= proposal.expires_ledger {
            proposal.status = ProposalStatus::Expired;
            save_proposal(&env, &proposal);
            env.events()
                .publish((symbol_short!("ms_expd"),), proposal_id);
            return Err(MultiSigError::ProposalExpired);
        }
        if proposal.signature_count < config.threshold {
            return Err(MultiSigError::ThresholdNotMet);
        }
        proposal.status = ProposalStatus::Approved;
        save_proposal(&env, &proposal);
        env.events().publish(
            (symbol_short!("ms_apprd"),),
            (proposal_id, proposal.signature_count),
        );
        Ok(proposal.status)
    }

    // ── Reads ───────────────────────────────────────────────────────────────

    /// Current signer set, threshold, and signature lifetime.
    pub fn get_config(env: Env) -> Result<MultiSigConfig, MultiSigError> {
        load_config(&env)
    }

    /// `true` when `address` is in the configured signer set — the membership
    /// half of signature validation, for callers that want to check before they
    /// start collecting.
    pub fn is_signer(env: Env, address: Address) -> Result<bool, MultiSigError> {
        let config = load_config(&env)?;
        Ok(is_signer_in(&config, &address))
    }

    /// `true` when `signer` has already approved `proposal_id`.
    pub fn has_approved(env: Env, proposal_id: Bytes, signer: Address) -> bool {
        env.storage()
            .persistent()
            .has(&approval_key(&proposal_id, &signer))
    }

    /// Ledger at which `signer` approved `proposal_id`, or `None` if they did
    /// not. This is the entire content of a "signature": who approved, and when.
    pub fn get_approval(env: Env, proposal_id: Bytes, signer: Address) -> Option<u32> {
        env.storage()
            .persistent()
            .get(&approval_key(&proposal_id, &signer))
    }

    /// How many distinct signatures a proposal has collected.
    pub fn get_signature_count(env: Env, proposal_id: Bytes) -> Result<u32, MultiSigError> {
        Ok(load_proposal(&env, &proposal_id)?.signature_count)
    }

    pub fn get_proposal(env: Env, proposal_id: Bytes) -> Result<Proposal, MultiSigError> {
        load_proposal(&env, &proposal_id)
    }

    /// `true` only for a proposal that reached the threshold and was finalized.
    pub fn is_approved(env: Env, proposal_id: Bytes) -> bool {
        env.storage()
            .persistent()
            .get::<DataKey, Proposal>(&DataKey::Proposal(proposal_id))
            .map(|p| p.status == ProposalStatus::Approved)
            .unwrap_or(false)
    }

    /// How many proposals have ever been opened.
    pub fn get_proposal_count(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::ProposalCount)
            .unwrap_or(0u32)
    }

    // ── Health monitoring (#678) and gradual rollout (#684) ──────────────
    pub fn health_check(env: Env) -> shared::health::HealthReport {
        let report = shared::health::health_check(&env);
        if report.anomaly {
            shared::rollout::maybe_auto_rollback(&env);
        }
        report
    }
    pub fn get_health_metrics(env: Env) -> shared::health::HealthMetrics {
        shared::health::get_metrics(&env)
    }
    pub fn get_sla_targets(env: Env) -> shared::health::SlaTargets {
        let _ = env;
        shared::health::sla_targets()
    }
    pub fn set_alert_config(env: Env, admin: Address, config: shared::health::AlertConfig) {
        admin.require_auth();
        shared::health::set_alert_config(&env, config);
    }
    pub fn get_alert_config(env: Env) -> shared::health::AlertConfig {
        shared::health::get_alert_config(&env)
    }
    pub fn detect_anomaly(env: Env) -> bool {
        shared::health::detect_anomaly(&env)
    }
    pub fn report_ok(env: Env, admin: Address) {
        admin.require_auth();
        shared::health::record_ok(&env);
    }
    pub fn report_error(env: Env, admin: Address) {
        admin.require_auth();
        shared::health::record_error(&env);
    }
    pub fn set_feature_flag(env: Env, admin: Address, flag: soroban_sdk::Symbol, enabled: bool) {
        admin.require_auth();
        shared::rollout::set_feature_flag(&env, &flag, enabled);
    }
    pub fn is_feature_enabled(env: Env, flag: soroban_sdk::Symbol) -> bool {
        shared::rollout::is_feature_enabled(&env, &flag)
    }
    pub fn set_canary_deployment(env: Env, admin: Address, canary: Address, stable: Address, canary_bps: u32) {
        admin.require_auth();
        shared::rollout::set_canary_deployment(&env, canary, stable, canary_bps);
    }
    pub fn route_to_canary(env: Env, caller: Address) -> bool {
        shared::rollout::route_to_canary(&env, &caller)
    }
    pub fn get_rollout_state(env: Env) -> shared::rollout::RolloutState {
        shared::rollout::get_state(&env)
    }
    pub fn set_rollback_trigger(env: Env, admin: Address, error_bps: u32) {
        admin.require_auth();
        shared::rollout::set_rollback_trigger(&env, error_bps);
    }
    pub fn should_rollback(env: Env) -> bool {
        shared::rollout::should_rollback(&env)
    }
    pub fn trigger_rollback(env: Env, admin: Address) {
        admin.require_auth();
        shared::rollout::trigger_rollback(&env, &admin);
    }
}
