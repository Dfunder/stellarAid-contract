#![no_std]

use soroban_sdk::{contract, contractclient, contractimpl, contracttype, token, Address, BytesN, Env, String, Symbol, Vec};
use shared::pagination::{self, PageInfo};
use shared::pause;
use shared::types::Withdrawal;

/// Code-constant ceiling on how many withdrawal *index* entries are retained per
/// campaign (#986). The canonical `Withdrawal(id)` record is always retained; only
/// this derived per-campaign index is capped.
pub const MAX_WITHDRAWALS_PER_CAMPAIGN: u32 = 500;

/// Effective per-campaign index cap until an admin overrides it (#986).
pub const DEFAULT_WITHDRAWALS_PER_CAMPAIGN: u32 = 100;

/// Hard cap on how many index entries one `prune_withdrawals_by_campaign` call
/// may remove (#986). Mirrors `analytics::PRUNE_MAX_BATCH`.
pub const PRUNE_MAX_BATCH: u32 = 100;

#[contractclient(name = "DonationContractClient")]
trait DonationContractTrait {
    fn get_total_raised(env: Env, campaign_id: u64) -> i128;
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Canonical withdrawal record — never pruned (RETENTION §1).
    Withdrawal(u64) = 0,
    /// Per-campaign index of withdrawal ids, oldest-first. This is the derived,
    /// bounded list; it no longer duplicates the full record (#986).
    WithdrawalsByCampaign(u64) = 1,
    Admin = 2,
    Initialized = 3,
    DonationContract = 4,
    /// Running total withdrawn per campaign — never pruned, the reconciliation
    /// figure (#986).
    WithdrawnAmount(u64) = 5,
    /// Admin-configurable per-campaign index cap, bounded above by
    /// [`MAX_WITHDRAWALS_PER_CAMPAIGN`] (#986).
    RetentionCap = 6,
}

#[contracttype]
#[derive(Clone)]
pub struct WithdrawalRequestedEvent {
    pub withdrawal_id: u64,
    pub campaign_id: u64,
    pub recipient: Address,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone)]
pub struct WithdrawalApprovedEvent {
    pub withdrawal_id: u64,
    pub tx_hash: BytesN<32>,
}

#[contracttype]
#[derive(Clone)]
pub struct WithdrawalRejectedEvent {
    pub withdrawal_id: u64,
    pub reason: String,
}

#[contract]
pub struct WithdrawalContract;

#[contractimpl]
impl WithdrawalContract {
    /// Initialize the withdrawal contract with an admin and donation contract address.
    /// Must be called once before any other operations.
    pub fn initialize(env: Env, admin: Address, donation_contract: Address) {
        admin.require_auth();
        if env.storage().instance().has(&DataKey::Initialized) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage().instance().set(&DataKey::DonationContract, &donation_contract);
        shared::version::seed(&env, env!("CARGO_PKG_VERSION"));
    }

    shared::impl_semver_queries!();

    /// Pause the contract, blocking all state-changing operations.
    pub fn pause(env: Env, admin: Address) {
        admin.require_auth();
        Self::ensure_admin(&env, &admin);
        pause::pause(&env, &admin);
    }

    /// Unpause the contract, restoring normal operations.
    pub fn unpause(env: Env, admin: Address) {
        admin.require_auth();
        Self::ensure_admin(&env, &admin);
        pause::unpause(&env, &admin);
    }

    /// Request a withdrawal from a campaign's raised funds.
    /// The campaign owner initiates this; an admin must approve it.
    pub fn request_withdrawal(env: Env, campaign_id: u64, owner: Address, amount: i128, recipient: Address) -> u64 {
        pause::require_not_paused(&env);
        owner.require_auth();
        let id = Self::next_withdrawal_id(&env);
        let withdrawal = Withdrawal {
            campaign_id,
            recipient: recipient.clone(),
            amount,
            approved: false,
        };
        env.storage().persistent().set(&DataKey::Withdrawal(id), &withdrawal);
        // #986: the per-campaign list stores only the id. The full record lives
        // once, at `Withdrawal(id)`, so storage is no longer doubled. The index is
        // ring-trimmed to the (bounded) retention cap on every write.
        let cap = Self::effective_retention_cap(&env);
        let mut index: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::WithdrawalsByCampaign(campaign_id))
            .unwrap_or(Vec::new(&env));
        index.push_back(id);
        let mut trimmed = 0u32;
        while index.len() > cap {
            index.pop_front();
            trimmed += 1;
        }
        env.storage()
            .persistent()
            .set(&DataKey::WithdrawalsByCampaign(campaign_id), &index);
        if trimmed > 0 {
            env.events().publish(
                (Symbol::new(&env, "prune"),),
                (campaign_id, trimmed, cap),
            );
        }
        env.events().publish((Symbol::new(&env, "withdrawal_requested"),), WithdrawalRequestedEvent {
            withdrawal_id: id,
            campaign_id,
            recipient,
            amount,
        });
        id
    }

    /// Approve a withdrawal request. Checks that the available balance
    /// (total raised minus already withdrawn) covers the requested amount.
    pub fn approve_withdrawal(env: Env, withdrawal_id: u64, admin: Address, token: Address) {
        pause::require_not_paused(&env);
        admin.require_auth();
        Self::ensure_admin(&env, &admin);

        let withdrawal = env.storage().persistent().get::<DataKey, Withdrawal>(&DataKey::Withdrawal(withdrawal_id)).unwrap();
        let campaign_id = withdrawal.campaign_id;

        let donation_contract: Address = env.storage().instance().get(&DataKey::DonationContract).unwrap();
        let donation_client = DonationContractClient::new(&env, &donation_contract);
        let total_raised = donation_client.get_total_raised(&campaign_id);

        let already_withdrawn = env.storage().persistent().get(&DataKey::WithdrawnAmount(campaign_id)).unwrap_or(0_i128);
        let available = total_raised - already_withdrawn;

        if withdrawal.amount > available {
            panic!("insufficient funds: requested exceeds available balance");
        }

        // #764: verify the contract's balance *before* any state is written,
        // and in the same invocation as the transfer below. The previous code
        // wrote `approved = true` and bumped `WithdrawnAmount` first, so a
        // short balance left a withdrawal marked approved and the running total
        // inflated with an amount that was never paid out.
        let token_client = token::Client::new(&env, &token);
        if token_client.balance(&env.current_contract_address()) < withdrawal.amount {
            // Named to match `campaign::balance::INSUFFICIENT_CONTRACT_BALANCE`
            // and the typed `Error::InsufficientContractBalance`.
            panic!("InsufficientContractBalance");
        }

        let mut updated = withdrawal.clone();
        updated.approved = true;
        env.storage().persistent().set(&DataKey::Withdrawal(withdrawal_id), &updated);

        env.storage().persistent().set(&DataKey::WithdrawnAmount(campaign_id), &(already_withdrawn + withdrawal.amount));

        token_client.transfer(&env.current_contract_address(), &withdrawal.recipient, &withdrawal.amount);

        let tx_hash = BytesN::from_array(&env, &[0u8; 32]);
        env.events().publish((Symbol::new(&env, "withdrawal_approved"),), WithdrawalApprovedEvent { withdrawal_id, tx_hash });
    }

    /// Reject a withdrawal request with a reason.
    pub fn reject_withdrawal(env: Env, withdrawal_id: u64, admin: Address, reason: String) {
        pause::require_not_paused(&env);
        admin.require_auth();
        Self::ensure_admin(&env, &admin);
        // #765: bound the reason before it is attached to the event payload.
        shared::validation::require_string_len(&reason, shared::validation::MAX_MEMO_LEN, "reason");
        let withdrawal = env.storage().persistent().get::<DataKey, Withdrawal>(&DataKey::Withdrawal(withdrawal_id)).unwrap();
        let _ = withdrawal;
        env.events().publish((Symbol::new(&env, "withdrawal_rejected"),), WithdrawalRejectedEvent { withdrawal_id, reason });
    }

    /// Get a withdrawal request by ID.
    pub fn get_withdrawal(env: Env, withdrawal_id: u64) -> Option<Withdrawal> {
        env.storage().persistent().get(&DataKey::Withdrawal(withdrawal_id))
    }

    /// One bounded page of a campaign's withdrawal history (#986, #803).
    ///
    /// Walks the per-campaign id index via
    /// [`shared::pagination::collect_window`] and loads the canonical record for
    /// each id in the page, so only the returned page's records are read. A
    /// pruned index position is skipped, which is why `count` can be below
    /// `limit`; the [`PageInfo`] tells a caller how to continue.
    pub fn get_withdrawals_by_campaign(
        env: Env,
        campaign_id: u64,
        start: u32,
        limit: u32,
    ) -> (Vec<Withdrawal>, PageInfo) {
        let index: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::WithdrawalsByCampaign(campaign_id))
            .unwrap_or(Vec::new(&env));
        let total = index.len();
        pagination::collect_window(&env, total, start, limit, |i| {
            let id = index.get(i)?;
            env.storage()
                .persistent()
                .get::<DataKey, Withdrawal>(&DataKey::Withdrawal(id))
        })
    }

    /// Current number of withdrawal ids retained in the per-campaign index
    /// (#998).
    ///
    /// Read-only and O(1): it reads the stored id index and returns its length
    /// without loading any canonical `Withdrawal` record, so it never triggers
    /// the write path. The index is ring-trimmed to the retention cap on every
    /// write, so this is the **current size against the cap** — compare with
    /// [`Self::get_withdrawal_retention_cap`] for utilisation. Storage tier:
    /// persistent (`DataKey::WithdrawalsByCampaign`).
    pub fn get_withdrawal_count(env: Env, campaign_id: u64) -> u32 {
        env.storage()
            .persistent()
            .get::<DataKey, Vec<u64>>(&DataKey::WithdrawalsByCampaign(campaign_id))
            .unwrap_or(Vec::new(&env))
            .len()
    }

    // ── Retention policy (#986) ────────────────────────────────────────────

    /// The cap currently applied to the per-campaign withdrawal index.
    pub fn get_withdrawal_retention_cap(env: Env) -> u32 {
        Self::effective_retention_cap(&env)
    }

    /// Read the retention policy: `(code_ceiling, effective_cap, prune_batch_cap)`.
    pub fn get_withdrawal_retention_policy(env: Env) -> (u32, u32, u32) {
        (
            MAX_WITHDRAWALS_PER_CAMPAIGN,
            Self::effective_retention_cap(&env),
            PRUNE_MAX_BATCH,
        )
    }

    /// Set the admin-configurable per-campaign index cap (#986).
    ///
    /// Must be in `1..=MAX_WITHDRAWALS_PER_CAMPAIGN`, so the code ceiling that
    /// bounds storage growth can never be raised away.
    pub fn set_withdrawal_retention_cap(env: Env, admin: Address, cap: u32) {
        admin.require_auth();
        Self::ensure_admin(&env, &admin);
        if cap == 0 || cap > MAX_WITHDRAWALS_PER_CAMPAIGN {
            panic!("retention cap out of range");
        }
        env.storage().instance().set(&DataKey::RetentionCap, &cap);
        env.events().publish((Symbol::new(&env, "retcap"),), cap);
    }

    /// Remove a bounded batch of the oldest **index** entries for a campaign
    /// (#986).
    ///
    /// Follows `analytics::prune_earnings`: `limit` is mandatory, capped at
    /// [`PRUNE_MAX_BATCH`], and bounds both removals and examinations;
    /// `upto_index` states the window; the admin must authenticate; and the scan
    /// stops at the first index entry whose withdrawal is **not yet approved**,
    /// so a pending (claimable) withdrawal is never dropped from the list.
    ///
    /// Only the derived index is touched. The canonical `Withdrawal(id)` record
    /// and the `WithdrawnAmount(campaign_id)` reconciliation total are never
    /// modified — a caller that still holds an id can always resolve the record
    /// through [`Self::get_withdrawal`].
    pub fn prune_withdrawals_by_campaign(
        env: Env,
        admin: Address,
        campaign_id: u64,
        upto_index: u32,
        limit: u32,
    ) -> u32 {
        admin.require_auth();
        Self::ensure_admin(&env, &admin);
        if upto_index == 0 || limit == 0 || limit > PRUNE_MAX_BATCH {
            panic!("invalid prune bounds");
        }

        let mut index: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::WithdrawalsByCampaign(campaign_id))
            .unwrap_or(Vec::new(&env));
        let target = upto_index.min(index.len()).min(limit);
        let mut removed = 0u32;
        while removed < target {
            let id = index.get(0).unwrap();
            let terminal = env
                .storage()
                .persistent()
                .get::<DataKey, Withdrawal>(&DataKey::Withdrawal(id))
                .map(|w| w.approved)
                .unwrap_or(true);
            if !terminal {
                break;
            }
            index.pop_front();
            removed += 1;
        }

        env.storage()
            .persistent()
            .set(&DataKey::WithdrawalsByCampaign(campaign_id), &index);
        env.events()
            .publish((Symbol::new(&env, "prune"),), (campaign_id, removed));
        removed
    }

    /// Upgrade the contract to a new WASM implementation.
    pub fn upgrade(env: Env, admin: Address, new_wasm_hash: BytesN<32>) {
        admin.require_auth();
        Self::ensure_admin(&env, &admin);
        env.deployer().update_current_contract_wasm(&new_wasm_hash);
    }

    /// Get the total amount already withdrawn from a campaign.
    pub fn get_withdrawn_amount(env: Env, campaign_id: u64) -> i128 {
        env.storage().persistent().get(&DataKey::WithdrawnAmount(campaign_id)).unwrap_or(0_i128)
    }

    fn ensure_admin(env: &Env, admin: &Address) {
        let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if stored_admin != *admin {
            panic!("unauthorized");
        }
    }

    /// The per-campaign index cap actually applied (#986): the stored admin value
    /// (or [`DEFAULT_WITHDRAWALS_PER_CAMPAIGN`]), clamped to
    /// [`MAX_WITHDRAWALS_PER_CAMPAIGN`].
    fn effective_retention_cap(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::RetentionCap)
            .unwrap_or(DEFAULT_WITHDRAWALS_PER_CAMPAIGN)
            .min(MAX_WITHDRAWALS_PER_CAMPAIGN)
    }

    fn next_withdrawal_id(env: &Env) -> u64 {
        let mut next_id: u64 = env.storage().instance().get(&Symbol::new(env, "next_withdrawal_id")).unwrap_or(1);
        env.storage().instance().set(&Symbol::new(env, "next_withdrawal_id"), &(next_id + 1));
        next_id
    }

// ── Health monitoring (#678) and gradual rollout (#684) ──────────────
// Authorization: every setter in this block is admin-only and is enforced by
// an identity check against the admin stored at initialization (#763).
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
        // #763: `admin.require_auth()` alone accepts *any* address. Without this
        // identity check against the stored admin, any account can reach this
        // entry point by naming itself as `admin`.
        Self::ensure_admin(&env, &admin);
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
        // #763: `admin.require_auth()` alone accepts *any* address. Without this
        // identity check against the stored admin, any account can reach this
        // entry point by naming itself as `admin`.
        Self::ensure_admin(&env, &admin);
        shared::health::record_ok(&env);
    }
    pub fn report_error(env: Env, admin: Address) {
        admin.require_auth();
        // #763: `admin.require_auth()` alone accepts *any* address. Without this
        // identity check against the stored admin, any account can reach this
        // entry point by naming itself as `admin`.
        Self::ensure_admin(&env, &admin);
        shared::health::record_error(&env);
    }
    pub fn set_feature_flag(env: Env, admin: Address, flag: soroban_sdk::Symbol, enabled: bool) {
        admin.require_auth();
        // #763: `admin.require_auth()` alone accepts *any* address. Without this
        // identity check against the stored admin, any account can reach this
        // entry point by naming itself as `admin`.
        Self::ensure_admin(&env, &admin);
        shared::rollout::set_feature_flag(&env, &flag, enabled);
    }
    pub fn is_feature_enabled(env: Env, flag: soroban_sdk::Symbol) -> bool {
        shared::rollout::is_feature_enabled(&env, &flag)
    }
    pub fn set_canary_deployment(env: Env, admin: Address, canary: Address, stable: Address, canary_bps: u32) {
        admin.require_auth();
        // #763: `admin.require_auth()` alone accepts *any* address. Without this
        // identity check against the stored admin, any account can reach this
        // entry point by naming itself as `admin`.
        Self::ensure_admin(&env, &admin);
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
        // #763: `admin.require_auth()` alone accepts *any* address. Without this
        // identity check against the stored admin, any account can reach this
        // entry point by naming itself as `admin`.
        Self::ensure_admin(&env, &admin);
        shared::rollout::set_rollback_trigger(&env, error_bps);
    }
    pub fn should_rollback(env: Env) -> bool {
        shared::rollout::should_rollback(&env)
    }
    pub fn trigger_rollback(env: Env, admin: Address) {
        admin.require_auth();
        // #763: `admin.require_auth()` alone accepts *any* address. Without this
        // identity check against the stored admin, any account can reach this
        // entry point by naming itself as `admin`.
        Self::ensure_admin(&env, &admin);
        shared::rollout::trigger_rollback(&env, &admin);
    }
}


#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{contract, contractimpl, testutils::Address as _, token, Address, Env};

    /// Minimal stand-in for the donation contract, exposing only the probe the
    /// withdrawal contract uses to size a campaign's available balance.
    #[contract]
    pub struct MockDonation;

    #[contractimpl]
    impl MockDonation {
        pub fn init(env: Env, total_raised: i128) {
            env.storage().instance().set(&0u32, &total_raised);
        }
        pub fn get_total_raised(env: Env, _campaign_id: u64) -> i128 {
            env.storage().instance().get(&0u32).unwrap()
        }
    }

    struct Fixture<'a> {
        env: Env,
        client: WithdrawalContractClient<'a>,
        contract_id: Address,
        token: Address,
        admin: Address,
        owner: Address,
        recipient: Address,
    }

    fn setup<'a>() -> Fixture<'a> {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let owner = Address::generate(&env);
        let recipient = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let token = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();

        let donation_id = env.register_contract(None, MockDonation);
        MockDonationClient::new(&env, &donation_id).init(&1_000_000);

        let contract_id = env.register_contract(None, WithdrawalContract);
        let client = WithdrawalContractClient::new(&env, &contract_id);
        client.initialize(&admin, &donation_id);

        // Fund the withdrawal contract so an approved transfer can settle.
        token::StellarAssetClient::new(&env, &token).mint(&contract_id, &1_000_000);

        Fixture {
            env,
            client,
            contract_id,
            token,
            admin,
            owner,
            recipient,
        }
    }

    #[test]
    fn withdrawal_requests_and_approval_flow() {
        let f = setup();
        let id = f.client.request_withdrawal(&7_u64, &f.owner, &120_i128, &f.recipient);

        let withdrawal = f.client.get_withdrawal(&id).unwrap();
        assert_eq!(withdrawal.amount, 120_i128);
        assert!(!withdrawal.approved);

        f.client.approve_withdrawal(&id, &f.admin, &f.token);
        assert!(f.client.get_withdrawal(&id).unwrap().approved);
        assert_eq!(f.client.get_withdrawn_amount(&7_u64), 120_i128);
        assert_eq!(
            token::Client::new(&f.env, &f.token).balance(&f.recipient),
            120
        );
    }

    /// The index stores only ids, and a lookup still resolves the full record
    /// through the canonical key (#986: no double storage).
    #[test]
    fn index_resolves_to_the_single_canonical_record() {
        let f = setup();
        let id = f.client.request_withdrawal(&7_u64, &f.owner, &50_i128, &f.recipient);

        let (page, info) = f.client.get_withdrawals_by_campaign(&7_u64, &0, &10);
        assert_eq!(page.len(), 1);
        assert_eq!(info.total, 1);
        assert_eq!(page.get(0).unwrap().amount, 50_i128);
        // Same record the canonical getter returns.
        assert_eq!(f.client.get_withdrawal(&id).unwrap().amount, 50_i128);
    }

    #[test]
    fn retention_cap_trims_the_index() {
        let f = setup();
        f.client.set_withdrawal_retention_cap(&f.admin, &3);
        for _ in 0..5 {
            f.client.request_withdrawal(&7_u64, &f.owner, &10_i128, &f.recipient);
        }

        let (page, info) = f.client.get_withdrawals_by_campaign(&7_u64, &0, &10);
        assert_eq!(page.len(), 3);
        assert_eq!(info.total, 3);
        assert_eq!(f.client.get_withdrawal_retention_cap(), 3);
    }

    /// Pruning removes index entries only, and never a pending (unapproved)
    /// withdrawal.
    #[test]
    fn prune_is_bounded_and_stops_at_a_pending_withdrawal() {
        let f = setup();
        let approved_id = f.client.request_withdrawal(&7_u64, &f.owner, &10_i128, &f.recipient);
        f.client.approve_withdrawal(&approved_id, &f.admin, &f.token);
        let pending_id = f.client.request_withdrawal(&7_u64, &f.owner, &10_i128, &f.recipient);

        // Only the approved (terminal) head can be pruned; the scan stops at the
        // pending record behind it.
        let removed = f
            .client
            .prune_withdrawals_by_campaign(&f.admin, &7_u64, &5, &5);
        assert_eq!(removed, 1);
        // The canonical record and reconciliation total survive regardless.
        assert!(f.client.get_withdrawal(&pending_id).is_some());
        assert_eq!(f.client.get_withdrawn_amount(&7_u64), 10_i128);
    }

    #[test]
    fn pause_blocks_withdrawal_requests() {
        let f = setup();
        f.client.pause(&f.admin);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.client.request_withdrawal(&7_u64, &f.owner, &120_i128, &f.recipient);
        }));
        assert!(result.is_err());

        f.client.unpause(&f.admin);
        let id = f.client.request_withdrawal(&7_u64, &f.owner, &120_i128, &f.recipient);
        assert_eq!(id, 1);
    }
}