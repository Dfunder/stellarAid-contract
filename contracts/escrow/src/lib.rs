//! Escrow Smart Contract
//!
//! Handles locking, releasing, refunding, and dispute escrow workflows for StellarAid.
//! Architecture Decision: [ADR-0002](../../docs/ADRs/0002-escrow-architecture-and-state-machine.md)
//! See also: [ADR-0007](../../docs/ADRs/0007-storage-data-model-and-ttl-management.md)

#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, token, Address, Bytes, Env};

pub mod errors;
pub mod storage;
pub mod cross_contract;

pub use cross_contract::{AtomicCommitMarker, AtomicCommitState};

use errors::EscrowError;
use storage::{CommissionStatus, EscrowRecord, escrow_exists, get_escrow, save_escrow};

include!("../../semver_types.rs");

/// Emit a correlated event `(domain, action, "corr")` carrying a deterministic
/// correlation id derived from the shared operation key (#661).
///
/// Off-chain indexers join these events on the id + key to reconstruct the
/// cross-contract trace of a logical operation.
fn correlation_publish(
    env: &Env,
    domain: &str,
    action: soroban_sdk::Symbol,
    id: &Bytes,
    key: &Bytes,
) {
    let scope = shared::correlation::scope(env, domain);
    let cid = shared::correlation::CorrelationId::derive(env, &scope, &[id]);
    shared::correlation::publish(env, soroban_sdk::Symbol::new(env, domain), action, &cid, key);
}

// ── Pause helpers (closes #594) ─────────────────────────────────────────────

/// Storage key for the escrow contract pause flag.
#[soroban_sdk::contracttype]
enum PauseKey {
    Paused,
    Admin,
}

/// Require the escrow contract is not paused.
fn require_not_paused(env: &Env) -> Result<(), EscrowError> {
    let paused: bool = env.storage().instance().get(&PauseKey::Paused).unwrap_or(false);
    if paused {
        Err(EscrowError::ContractPaused)
    } else {
        Ok(())
    }
}

/// Ledgers until an escrow record expires from persistent storage (~30 days at 6s/ledger).
/// Closes #487 – ledger-based TTL for escrow records.
/// Disputed escrows are extended with the configurable dispute-period TTL
/// instead (see [`EscrowContract::set_dispute_ttl_ledgers`], #586).
const ESCROW_TTL_LEDGERS: u32 = 432_000;

fn extend_escrow_ttl(env: &Env, record: &EscrowRecord, ledgers: u32) {
    use storage::DataKey;
    env.storage().persistent().extend_ttl(
        &DataKey::Escrow(record.commission_id.clone()),
        ledgers,
        ledgers,
    );
}

fn extend_escrow_ttl_default(env: &Env, record: &EscrowRecord) {
    extend_escrow_ttl(env, record, ESCROW_TTL_LEDGERS);
}

/// Overflow-safe fee split (#588).
///
/// Computes `(fee, payout)` from `amount` and `fee_bps` using checked
/// arithmetic. Any intermediate overflow returns [`EscrowError::ArithmeticOverflow`]
/// instead of silently zeroing the fee or aborting the transaction.
fn calculate_fee_split(amount: i128, fee_bps: u32) -> Result<(i128, i128), EscrowError> {
    let product = amount
        .checked_mul(fee_bps as i128)
        .ok_or(EscrowError::ArithmeticOverflow)?;
    let fee = product / 10_000;
    let payout = amount
        .checked_sub(fee)
        .ok_or(EscrowError::ArithmeticOverflow)?;
    Ok((fee, payout))
}

#[contract]
pub struct EscrowContract;

#[contractimpl]
impl EscrowContract {
    // ── Pause / Resume (closes #594) ────────────────────────────────────────

    /// Initialise the escrow admin. Must be called once after deployment.
    /// If `initialize` is never called, pause/unpause are unavailable.
    pub fn initialize(env: Env, admin: Address) -> Result<(), EscrowError> {
        admin.require_auth();
        if env.storage().instance().has(&PauseKey::Admin) {
            return Err(EscrowError::AlreadyExists);
        }
        env.storage().instance().set(&PauseKey::Admin, &admin);
        env.storage().instance().set(&PauseKey::Paused, &false);
        Ok(())
    }

    /// Return the contract semantic version (MAJOR.MINOR.PATCH) from Cargo.toml.
    pub fn get_version(_env: Env) -> ContractVersion {
        parse_pkg_semver(env!("CARGO_PKG_VERSION"))
    }

    /// Return crate name, semver, min-compatible client version, and storage schema.
    pub fn get_version_metadata(env: Env) -> VersionMetadata {
        let version = parse_pkg_semver(env!("CARGO_PKG_VERSION"));
        VersionMetadata {
            name: soroban_sdk::String::from_str(&env, env!("CARGO_PKG_NAME")),
            min_compatible: min_compatible_for(&version),
            version,
            storage_schema: CURRENT_STORAGE_SCHEMA,
        }
    }

    /// Return `true` if this WASM can serve a client that requires `(major, minor, patch)`.
    pub fn is_version_compatible(_env: Env, major: u32, minor: u32, patch: u32) -> bool {
        is_compatible(
            &parse_pkg_semver(env!("CARGO_PKG_VERSION")),
            &ContractVersion {
                major,
                minor,
                patch,
            },
        )
    }

    /// Pause the escrow contract — blocks `create_escrow` and `refund_client`.
    /// Only callable by the escrow admin set during `initialize`.
    /// Closes #594.
    pub fn pause(env: Env, admin: Address) -> Result<(), EscrowError> {
        admin.require_auth();
        let stored: Address = env.storage().instance()
            .get(&PauseKey::Admin)
            .ok_or(EscrowError::Unauthorized)?;
        if stored != admin {
            return Err(EscrowError::Unauthorized);
        }
        env.storage().instance().set(&PauseKey::Paused, &true);
        env.events().publish(
            (symbol_short!("esc"), symbol_short!("paused")),
            admin,
        );
        Ok(())
    }

    /// Resume normal operations after a pause. Only callable by the escrow admin.
    /// Closes #594.
    pub fn unpause(env: Env, admin: Address) -> Result<(), EscrowError> {
        admin.require_auth();
        let stored: Address = env.storage().instance()
            .get(&PauseKey::Admin)
            .ok_or(EscrowError::Unauthorized)?;
        if stored != admin {
            return Err(EscrowError::Unauthorized);
        }
        env.storage().instance().set(&PauseKey::Paused, &false);
        env.events().publish(
            (symbol_short!("esc"), symbol_short!("unpaused")),
            admin,
        );
        Ok(())
    }

    /// Returns `true` when the contract is currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage().instance().get(&PauseKey::Paused).unwrap_or(false)
    }

    /// Closes #482 (CEI), #486 (events), #487 (TTL), #587 (reentrancy guard).
    /// Closes #594 (pause guard on create_escrow).
    /// CEI: Checks → Effects (save record) → Interactions (token transfer).
    pub fn create_escrow(
        env: Env,
        commission_id: Bytes,
        client: Address,
        artist: Address,
        amount: i128,
        config_contract: Address,
    ) -> Result<(), EscrowError> {
        client.require_auth();

        // CHECKS
        require_not_paused(&env)?;
        if amount <= 0 { return Err(EscrowError::InvalidAmount); }
        if escrow_exists(&env, &commission_id) { return Err(EscrowError::AlreadyExists); }

        let fee_bps: u32 = env.invoke_contract(
            &config_contract, &symbol_short!("get_fee_b"), soroban_sdk::vec![&env],
        );
        let usdc_token: Address = env.invoke_contract(
            &config_contract, &symbol_short!("get_usdc"), soroban_sdk::vec![&env],
        );

        // EFFECTS + INTERACTIONS under re-entrancy guard (#587);
        // CEI: persist before the external token transfer.
        storage::with_reentrancy_guard(&env, || {
            let record = EscrowRecord {
                commission_id: commission_id.clone(),
                client: client.clone(),
                artist: artist.clone(),
                amount,
                fee_bps,
                status: CommissionStatus::Locked,
                created_ledger: env.ledger().sequence(),
            };
            save_escrow(&env, &record);
            extend_escrow_ttl_default(&env, &record);

            // INTERACTIONS – external call after effects
            token::Client::new(&env, &usdc_token).transfer(
                &client, &env.current_contract_address(), &amount,
            );

            // EVENT
            env.events().publish(
                (symbol_short!("escrow"), symbol_short!("created")),
                (commission_id.clone(), amount),
            );
            correlation_publish(&env, "escrow", symbol_short!("created"), &commission_id, &commission_id);
            Ok(())
        })
    }

    /// Closes #482 (CEI), #486 (events), #588 (overflow-safe fees), #587 (reentrancy guard).
    /// CEI: Checks → Effects (status update) → Interactions (transfers).
    pub fn release_payment(
        env: Env,
        commission_id: Bytes,
        config_contract: Address,
    ) -> Result<(), EscrowError> {
        // CHECKS
        let mut r = get_escrow(&env, &commission_id);
        if r.status != CommissionStatus::Locked { return Err(EscrowError::InvalidStatus); }

        let admin: Address = env.invoke_contract(&config_contract, &symbol_short!("get_adm"), soroban_sdk::vec![&env]);
        admin.require_auth();
        let usdc: Address = env.invoke_contract(&config_contract, &symbol_short!("get_usdc"), soroban_sdk::vec![&env]);
        let pw: Address = env.invoke_contract(&config_contract, &symbol_short!("get_pw"), soroban_sdk::vec![&env]);

        let artist = r.artist.clone();

        storage::with_reentrancy_guard(&env, || {
            let (fee, payout) = calculate_fee_split(r.amount, r.fee_bps)?;

            // EFFECTS
            r.status = CommissionStatus::Released;
            save_escrow(&env, &r);

            // INTERACTIONS
            let tc = token::Client::new(&env, &usdc);
            tc.transfer(&env.current_contract_address(), &artist, &payout);
            tc.transfer(&env.current_contract_address(), &pw, &fee);

            // EVENT
            env.events().publish(
                (symbol_short!("escrow"), symbol_short!("released")),
                (commission_id.clone(), payout, fee),
            );
            Ok(())
        })
    }

    /// Closes #482 (CEI), #486 (events), #587 (reentrancy guard).
    /// Closes #594 (pause guard on refund_client).
    /// CEI: Checks → Effects (status update) → Interactions (transfer).
    pub fn refund_client(
        env: Env,
        commission_id: Bytes,
        config_contract: Address,
    ) -> Result<(), EscrowError> {
        // CHECKS
        require_not_paused(&env)?;
        let mut r = get_escrow(&env, &commission_id);
        if r.status != CommissionStatus::Locked && r.status != CommissionStatus::Disputed {
            return Err(EscrowError::InvalidStatus);
        }
        let admin: Address = env.invoke_contract(&config_contract, &symbol_short!("get_adm"), soroban_sdk::vec![&env]);
        admin.require_auth();
        let usdc: Address = env.invoke_contract(&config_contract, &symbol_short!("get_usdc"), soroban_sdk::vec![&env]);

        let client = r.client.clone();
        let amount = r.amount;

        storage::with_reentrancy_guard(&env, || {
            // EFFECTS
            r.status = CommissionStatus::Refunded;
            save_escrow(&env, &r);

            // INTERACTIONS
            token::Client::new(&env, &usdc).transfer(&env.current_contract_address(), &client, &amount);

            // EVENT
            env.events().publish(
                (symbol_short!("escrow"), symbol_short!("refunded")),
                (commission_id.clone(), client.clone(), amount),
            );
            Ok(())
        })
    }

    /// Closes #482 (CEI), #486 (events).
    pub fn expire_escrow(env: Env, commission_id: Bytes, expiry_ledger: u32) -> Result<(), EscrowError> {
        // CHECKS
        let mut r = get_escrow(&env, &commission_id);
        if r.status != CommissionStatus::Locked { return Err(EscrowError::InvalidStatus); }
        if env.ledger().sequence() < expiry_ledger { return Err(EscrowError::NotExpired); }

        // EFFECTS
        r.status = CommissionStatus::Expired;
        save_escrow(&env, &r);

        // EVENT
        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("expired")),
            (commission_id, expiry_ledger),
        );
        Ok(())
    }

    /// Closes #482 (CEI), #486 (events), #586 (dispute-period TTL extension).
    pub fn open_dispute(env: Env, commission_id: Bytes, initiator: Address) -> Result<(), EscrowError> {
        initiator.require_auth();

        // CHECKS
        let mut r = get_escrow(&env, &commission_id);
        if r.status == CommissionStatus::Disputed { return Err(EscrowError::DisputeAlreadyOpen); }
        if r.status != CommissionStatus::Locked { return Err(EscrowError::InvalidStatus); }
        if initiator != r.client && initiator != r.artist { return Err(EscrowError::Unauthorized); }

        // EFFECTS – extend the record TTL with the dispute-period length so the
        // escrow cannot expire mid-arbitration (#586).
        r.status = CommissionStatus::Disputed;
        save_escrow(&env, &r);
        extend_escrow_ttl(&env, &r, storage::get_dispute_ttl_ledgers(&env));

        // EVENT
        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("disputed")),
            (commission_id, initiator),
        );
        Ok(())
    }

    /// Configure the dispute-period TTL extension in ledgers (#586).
    ///
    /// Only the platform admin (as resolved from `config_contract`) may call
    /// this. The value is used by `open_dispute` to extend a disputed escrow's
    /// persistent-storage TTL so it survives arbitration.
    pub fn set_dispute_ttl_ledgers(
        env: Env,
        config_contract: Address,
        ledgers: u32,
    ) -> Result<(), EscrowError> {
        let admin: Address = env.invoke_contract(
            &config_contract, &symbol_short!("get_adm"), soroban_sdk::vec![&env],
        );
        admin.require_auth();

        // CHECKS
        if ledgers == 0 || ledgers < ESCROW_TTL_LEDGERS {
            return Err(EscrowError::InvalidAmount);
        }

        // EFFECTS
        storage::set_dispute_ttl_ledgers(&env, ledgers);

        // EVENT
        env.events().publish(
            (symbol_short!("ttl"), symbol_short!("updated")),
            ledgers,
        );
        Ok(())
    }

    /// Returns the currently configured dispute-period TTL in ledgers (#586).
    pub fn get_dispute_ttl_ledgers(env: Env) -> u32 {
        storage::get_dispute_ttl_ledgers(&env)
    }

    /// Settle a cancelled commission (#605).
    ///
    /// The split is computed off-contract by the commission agreement's
    /// pro-rata settlement and passed in; the two amounts must account for the
    /// escrowed total exactly, so no dust can be stranded. The platform fee is
    /// charged only on the artist's share — the client's refund is not taxed.
    ///
    /// CEI: Checks → Effects (status update) → Interactions (transfers).
    pub fn cancel_escrow(
        env: Env,
        commission_id: Bytes,
        config_contract: Address,
        artist_amount: i128,
        client_refund: i128,
    ) -> Result<(), EscrowError> {
        // CHECKS
        let mut r = get_escrow(&env, &commission_id);
        if r.status != CommissionStatus::Locked && r.status != CommissionStatus::Disputed {
            return Err(EscrowError::InvalidStatus);
        }
        if artist_amount < 0 || client_refund < 0 {
            return Err(EscrowError::InvalidAmount);
        }
        let total = artist_amount
            .checked_add(client_refund)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        if total != r.amount {
            return Err(EscrowError::InvalidSplit);
        }

        let admin: Address = env.invoke_contract(&config_contract, &symbol_short!("get_adm"), soroban_sdk::vec![&env]);
        admin.require_auth();
        let usdc: Address = env.invoke_contract(&config_contract, &symbol_short!("get_usdc"), soroban_sdk::vec![&env]);
        let pw: Address = env.invoke_contract(&config_contract, &symbol_short!("get_pw"), soroban_sdk::vec![&env]);

        let artist = r.artist.clone();
        let client = r.client.clone();

        storage::with_reentrancy_guard(&env, || {
            let (fee, payout) = calculate_fee_split(artist_amount, r.fee_bps)?;

            // EFFECTS
            r.status = CommissionStatus::Cancelled;
            save_escrow(&env, &r);

            // INTERACTIONS
            let tc = token::Client::new(&env, &usdc);
            if payout > 0 {
                tc.transfer(&env.current_contract_address(), &artist, &payout);
            }
            if fee > 0 {
                tc.transfer(&env.current_contract_address(), &pw, &fee);
            }
            if client_refund > 0 {
                tc.transfer(&env.current_contract_address(), &client, &client_refund);
            }

            // EVENT
            env.events().publish(
                (symbol_short!("escrow"), symbol_short!("cancelled")),
                (commission_id.clone(), payout, fee, client_refund),
            );
            Ok(())
        })
    }

    pub fn get_escrow(env: Env, commission_id: Bytes) -> Result<EscrowRecord, EscrowError> {
        if !escrow_exists(&env, &commission_id) { return Err(EscrowError::NotFound); }
        Ok(storage::get_escrow(&env, &commission_id))
    }

    // ── Atomic escrow-to-commission flow (closes #656) ────────────────────
    //
    // Public entry points over the orchestration in `cross_contract`. The
    // multi-step begin/confirm/finalize/rollback markers coordinate the two
    // sides across transactions; `atomic_escrow_to_commission` performs the
    // single all-or-nothing fund migration guard with the host's atomicity.

    /// Open a commit marker for an escrow that will migrate into a commission.
    pub fn begin_atomic_commit(
        env: Env,
        commission_id: Bytes,
    ) -> Result<AtomicCommitMarker, EscrowError> {
        cross_contract::begin_atomic_commit(&env, &commission_id)
    }

    /// Record one participant’s readiness confirmation.
    pub fn confirm_atomic_step(
        env: Env,
        commission_id: Bytes,
        from: Address,
    ) -> Result<u32, EscrowError> {
        cross_contract::confirm_atomic_step(&env, &commission_id, from)
    }

    /// Aware both sides confirmed: mark the flow settled and the escrow released.
    pub fn finalize_atomic_commit(
        env: Env,
        commission_id: Bytes,
    ) -> Result<AtomicCommitMarker, EscrowError> {
        cross_contract::finalize_atomic_commit(&env, &commission_id)
    }

    /// Abandon the flow before any funds moved; escrow stays intact.
    pub fn rollback_atomic_commit(
        env: Env,
        commission_id: Bytes,
    ) -> Result<AtomicCommitMarker, EscrowError> {
        cross_contract::rollback_atomic_commit(&env, &commission_id)
    }

    /// Read the current commit marker.
    pub fn get_atomic_commit(
        env: Env,
        commission_id: Bytes,
    ) -> Result<AtomicCommitMarker, EscrowError> {
        if !storage::atomic_marker_exists(&env, &commission_id) {
            return Err(EscrowError::NotFound);
        }
        Ok(storage::get_atomic_marker(&env, &commission_id))
    }

    /// Cross-contract consistency probe: does the commission agreement expect
    /// exactly `expected_amount` escrowed for this id?
    pub fn verify_agreement_consistency(
        env: Env,
        commission_contract: Address,
        commission_id: Bytes,
        expected_amount: i128,
    ) -> Result<bool, EscrowError> {
        cross_contract::verify_agreement_consistency(
            &env,
            &commission_contract,
            &commission_id,
            expected_amount,
        )
    }

    /// Atomic, single-transaction escrow→commission migration.
    ///
    /// Verifies the commission side agrees with the escrowed amount, then moves
    /// the escrowed balance (minus the platform fee) to the commission contract
    /// and the fee to the platform wallet. Any failed check or transfer aborts
    /// the whole transaction — nothing is left half-migrated (#656).
    pub fn atomic_escrow_to_commission(
        env: Env,
        commission_id: Bytes,
        config_contract: Address,
        commission_contract: Address,
    ) -> Result<AtomicCommitMarker, EscrowError> {
        cross_contract::atomic_escrow_to_commission(
            &env,
            &commission_id,
            &config_contract,
            &commission_contract,
        )
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


#[cfg(test)]
mod tests;
#[cfg(test)]
mod refund_tests;
#[cfg(test)]
mod dispute_tests;
#[cfg(test)]
mod fee_math_tests;
#[cfg(test)]
mod storage_edge_tests;
#[cfg(test)]
mod cancellation_tests;
#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod atomic_flow_tests;
#[cfg(test)]
mod correlation_tests;
