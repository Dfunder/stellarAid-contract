#![no_std]

use soroban_sdk::{contract, contractclient, contractimpl, contracttype, token, Address, BytesN, Env, String, Symbol, Vec};
use shared::types::{Campaign, CampaignStatus, Donation, DonationRefundedEvent, AnonymousDonationEvent};
use shared::guard;
use shared::pause;
use shared::pagination::{self, PageInfo};

/// Code-constant ceiling on the number of donation records retained per
/// campaign (and per donor). An admin-configurable retention cap may be set
/// below this value but never above it (#986).
pub const MAX_DONATION_HISTORY: u32 = 500;

/// Effective retention cap applied until an admin overrides it (#986).
pub const DEFAULT_DONATION_HISTORY: u32 = 100;

/// Hard cap on how many records one `prune_campaign_donations` call may
/// remove. Mirrors `analytics::PRUNE_MAX_BATCH` so every prune in the workspace
/// is bounded by the same order of magnitude (#986).
pub const PRUNE_MAX_BATCH: u32 = 100;

/// Retention window, in seconds, below which a donation record is never removed
/// by `prune_campaign_donations` (#986). Expressed against `Donation::timestamp`
/// rather than a ledger number — it mirrors `analytics::MIN_RETENTION_LEDGERS`
/// (~90 days), the window the analytics prune uses.
pub const MIN_RETENTION_SECONDS: u64 = 7_776_000;

#[contractclient(name = "CampaignContractClient")]
trait CampaignContractTrait {
    fn update_raised(env: Env, campaign_id: u64, amount: i128);
    /// Registers the donor so the campaign contract can enforce the
    /// "donor or creator" rule for disputes (#763).
    fn record_donor(env: Env, campaign_id: u64, donor: Address);
    /// Canonical donation entry point: debits `raised` and emits
    /// `donation_received` (#756). Preferred over `update_raised` because the
    /// event carries the donor, the asset code and the new running total.
    fn record_donation(
        env: Env,
        campaign_id: u64,
        donor: Address,
        amount: i128,
        asset_code: String,
    ) -> i128;
    fn get_campaign(env: Env, campaign_id: u64) -> Option<Campaign>;
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin = 0,
    DonationHistory(Address) = 1,
    CampaignDonations(u64) = 2,
    CampaignRaised(u64) = 3,
    CampaignContract = 4,
    Initialized = 5,
    Nonce(Address, u64) = 6,
    /// Admin-configurable retention cap, bounded above by
    /// [`MAX_DONATION_HISTORY`] (#986).
    RetentionCap = 7,
}

#[contracttype]
#[derive(Clone)]
pub struct DonationMadeEvent {
    pub donor: Address,
    pub campaign_id: u64,
    pub amount: i128,
}

/// Drop the oldest records from `list` until it is at most `cap` long (#986).
///
/// Donation lists are appended oldest-first, so the oldest record is always at
/// the front. Returns the trimmed list and the number of records dropped, so the
/// caller can publish a `(prune)` signal exactly when the cap is reached.
fn ring_trim(mut list: Vec<Donation>, cap: u32) -> (Vec<Donation>, u32) {
    let mut removed = 0u32;
    while list.len() > cap {
        list.pop_front();
        removed += 1;
    }
    (list, removed)
}

#[contract]
pub struct DonationContract;
#[contractimpl]
impl DonationContract {
    pub fn initialize(env: Env, admin: Address, campaign_contract: Address) {
        admin.require_auth();
        if env.storage().instance().has(&DataKey::Initialized) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::CampaignContract, &campaign_contract);
        env.storage().instance().set(&DataKey::Initialized, &true);
        shared::version::seed(&env, env!("CARGO_PKG_VERSION"));
    }

    shared::impl_semver_queries!();

    pub fn pause(env: Env, admin: Address) {
        admin.require_auth();
        Self::ensure_admin(&env, &admin);
        pause::pause(&env, &admin);
    }

    pub fn unpause(env: Env, admin: Address) {
        admin.require_auth();
        Self::ensure_admin(&env, &admin);
        pause::unpause(&env, &admin);
    }

    /// Record a donation and forward it to the campaign contract.
    ///
    /// `asset_code` is the human-readable code of the received asset (for
    /// example `USDC`). It is forwarded verbatim into the campaign contract's
    /// `donation_received` event (#756) and must already have been bounded by
    /// the caller; the campaign contract validates the length it depends on.
    pub fn donate(
        env: Env,
        donor: Address,
        campaign_id: u64,
        amount: i128,
        token: Address,
        asset_code: String,
        anonymous: bool,
        memo: Option<String>,
    ) {
        // Guard first (#762): a donation touches the token contract and then
        // writes three storage entries, so a re-entrant call must be refused
        // before any of them is applied.
        guard::acquire(&env);
        pause::require_not_paused(&env);
        if !anonymous {
            donor.require_auth();
        }
        if amount <= 0 {
            panic!("amount must be positive");
        }

        let campaign_contract: Address = env.storage().instance().get(&DataKey::CampaignContract).unwrap();
        let campaign_client = CampaignContractClient::new(&env, &campaign_contract);
        let campaign = campaign_client.get_campaign(&campaign_id).unwrap_or_else(|| panic!("campaign not found"));
        if campaign.status != CampaignStatus::Active {
            panic!("campaign is not active");
        }

        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&donor, &env.current_contract_address(), &amount);

        let effective_donor = if anonymous {
            Address::generate(&env)
        } else {
            donor.clone()
        };

        // #765: the memo is persisted inside `Donation`, so it must be bounded
        // before the write. An unbounded per-donation string is a permanent
        // storage liability for whoever sent it.
        let memo = match memo {
            Some(m) => {
                if m.len() > shared::validation::MAX_MEMO_LEN {
                    panic!(
                        "memo exceeds maximum length of {} bytes",
                        shared::validation::MAX_MEMO_LEN
                    );
                }
                Some(m)
            }
            None => None,
        };

        let timestamp = env.ledger().timestamp();
        let donation = Donation {
            donor: effective_donor.clone(),
            campaign_id,
            amount,
            timestamp,
            memo: memo.clone(),
            anonymous,
            token_address: Some(token),
        };

        // #986: both lists are ring-trimmed to the admin-configurable retention
        // cap on every write. The code ceiling (`MAX_DONATION_HISTORY`) can never
        // be exceeded, so a popular campaign can no longer grow an unbounded
        // vector that every later read has to pay for.
        let cap = Self::effective_retention_cap(&env);

        let mut donations = env.storage().persistent().get(&DataKey::CampaignDonations(campaign_id)).unwrap_or(Vec::new(&env));
        donations.push_back(donation.clone());
        let (donations, trimmed_campaign) = ring_trim(donations, cap);
        env.storage().persistent().set(&DataKey::CampaignDonations(campaign_id), &donations);
        if trimmed_campaign > 0 {
            env.events().publish(
                (Symbol::new(&env, "prune"),),
                (campaign_id, trimmed_campaign, cap),
            );
        }

        if !anonymous {
            let mut history = env.storage().persistent().get(&DataKey::DonationHistory(donor.clone())).unwrap_or(Vec::new(&env));
            history.push_back(donation.clone());
            let (history, _) = ring_trim(history, cap);
            env.storage().persistent().set(&DataKey::DonationHistory(donor), &history);
        }

        // Checked addition (#760): a wrapped running total would report a
        // balance the contract does not hold.
        let total = env
            .storage()
            .persistent()
            .get(&DataKey::CampaignRaised(campaign_id))
            .unwrap_or(0_i128);
        let new_total = total
            .checked_add(amount)
            .expect("campaign raised total overflow");
        env.storage()
            .persistent()
            .set(&DataKey::CampaignRaised(campaign_id), &new_total);

        campaign_client.update_raised(&campaign_id, &amount);
        // Register the donor only after the transfer has settled, so the
        // "donor or creator" dispute rule cannot be claimed without paying (#763).
        campaign_client.record_donor(&campaign_id, &effective_donor);
        guard::release(&env);
        // `record_donation` updates the campaign's `raised` total and emits
        // `donation_received` from the campaign contract, so the event and the
        // stored total can never drift apart (#756).
        campaign_client.record_donation(
            &campaign_id,
            &effective_donor,
            &amount,
            &asset_code,
        );

        if anonymous {
            env.events().publish(
                (Symbol::new(&env, "anonymous_donation"),),
                AnonymousDonationEvent {
                    campaign_id,
                    amount,
                },
            );
        } else {
            env.events().publish(
                (Symbol::new(&env, "donation_made"),),
                DonationMadeEvent {
                    donor: effective_donor,
                    campaign_id,
                    amount,
                },
            );
        }
    }

    /// Idempotency-guarded donation: rejects duplicate (donor, nonce) pairs.
    pub fn donate_with_nonce(
        env: Env,
        donor: Address,
        campaign_id: u64,
        amount: i128,
        token: Address,
        asset_code: String,
        anonymous: bool,
        memo: Option<String>,
        nonce: u64,
    ) {
        if env.storage().instance().has(&DataKey::Nonce(donor.clone(), nonce)) {
            panic!("nonce already used");
        }
        env.storage().instance().set(&DataKey::Nonce(donor.clone(), nonce), &true);
        Self::donate(env, donor, campaign_id, amount, token, asset_code, anonymous, memo);
    }

    pub fn refund(env: Env, caller: Address, campaign_id: u64, donor: Address, amount: i128, token: Address) {
        // Guard first (#762): this is the contract's `request_refund` path and
        // it moves funds out of the contract.
        guard::acquire(&env);
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        let campaign_contract: Address = env.storage().instance().get(&DataKey::CampaignContract).unwrap();
        let campaign_client = CampaignContractClient::new(&env, &campaign_contract);
        let campaign = campaign_client.get_campaign(&campaign_id).unwrap_or_else(|| panic!("campaign not found"));
        if campaign.status != CampaignStatus::Rejected {
            panic!("refund only allowed for rejected campaigns");
        }
        if caller != admin && caller != campaign.owner {
            panic!("unauthorized");
        }

        let total = env.storage().persistent().get(&DataKey::CampaignRaised(campaign_id)).unwrap_or(0_i128);
        if amount > total {
            panic!("refund amount exceeds total raised");
        }
        // Checked subtraction (#760).
        let new_total = total.checked_sub(amount).expect("campaign raised total underflow");
        env.storage()
            .persistent()
            .set(&DataKey::CampaignRaised(campaign_id), &new_total);

        // #764: verify the contract's balance before the transfer. The debit
        // above is already persisted at this point, so an unverified transfer
        // would leave the running total short of what the donor is owed.
        let token_client = token::Client::new(&env, &token);
        if token_client.balance(&env.current_contract_address()) < amount {
            // Named to match `campaign::balance::INSUFFICIENT_CONTRACT_BALANCE`
            // and the typed `Error::InsufficientContractBalance`.
            panic!("InsufficientContractBalance");
        }
        token_client.transfer(&env.current_contract_address(), &donor, &amount);

        guard::release(&env);
        env.events().publish(
            (Symbol::new(&env, "donation_refunded"),),
            DonationRefundedEvent {
                campaign_id,
                donor,
                amount,
                caller,
            },
        );
    }

    /// One bounded page of a campaign's donation history (#986, #802).
    ///
    /// The list is ordered oldest-first and already ring-trimmed to the
    /// retention cap on write, so a page beyond the retained window is empty
    /// rather than a trap. The [`PageInfo`] makes a truncated/empty window
    /// distinguishable from an empty campaign.
    pub fn get_donations_for_campaign(env: Env, campaign_id: u64, start: u32, limit: u32) -> (Vec<Donation>, PageInfo) {
        let donations = env.storage().persistent().get(&DataKey::CampaignDonations(campaign_id)).unwrap_or(Vec::new(&env));
        pagination::paginated(&env, &donations, start, limit)
    }

    pub fn get_total_raised(env: Env, campaign_id: u64) -> i128 {
        env.storage().persistent().get(&DataKey::CampaignRaised(campaign_id)).unwrap_or(0_i128)
    }

    /// Current number of donation records retained for `campaign_id` (#998).
    ///
    /// Read-only and O(1): it reads the stored list and returns its length
    /// without walking it. Because the list is ring-trimmed to the retention
    /// cap on every write, this is the **current size against the cap** rather
    /// than a lifetime total — an operator sees utilisation before a write
    /// starts failing. Storage tier: persistent
    /// (`DataKey::CampaignDonations`). Compare against
    /// [`Self::get_donation_retention_cap`] for the utilisation fraction.
    pub fn get_donation_count(env: Env, campaign_id: u64) -> u32 {
        env.storage()
            .persistent()
            .get::<DataKey, Vec<Donation>>(&DataKey::CampaignDonations(campaign_id))
            .unwrap_or(Vec::new(&env))
            .len()
    }

    /// One bounded page of a donor's donation history across all campaigns
    /// (#986). Same retention cap and page contract as
    /// [`Self::get_donations_for_campaign`].
    pub fn get_donor_history(env: Env, donor: Address, start: u32, limit: u32) -> (Vec<Donation>, PageInfo) {
        let history = env.storage().persistent().get(&DataKey::DonationHistory(donor)).unwrap_or(Vec::new(&env));
        pagination::paginated(&env, &history, start, limit)
    }

    /// Current number of records retained in `donor`'s history (#998).
    ///
    /// O(1) length read of `DataKey::DonationHistory`. Persistent tier; the
    /// same current-size-against-cap meaning as
    /// [`Self::get_donation_count`].
    pub fn get_donor_history_count(env: Env, donor: Address) -> u32 {
        env.storage()
            .persistent()
            .get::<DataKey, Vec<Donation>>(&DataKey::DonationHistory(donor))
            .unwrap_or(Vec::new(&env))
            .len()
    }

    // ── Retention policy (#986) ────────────────────────────────────────────

    /// The cap currently applied to both donation lists.
    ///
    /// Defaults to [`DEFAULT_DONATION_HISTORY`] until an admin overrides it and
    /// is always clamped to [`MAX_DONATION_HISTORY`], so the code ceiling holds
    /// even if a corrupt value were somehow stored.
    pub fn get_donation_retention_cap(env: Env) -> u32 {
        Self::effective_retention_cap(&env)
    }

    /// Read the retention policy: `(code_ceiling, effective_cap, prune_batch_cap)`.
    ///
    /// Mirrors `analytics::get_retention_policy` (#651): an operator can read
    /// what a write/prune is bounded by without reading the source.
    pub fn get_donation_retention_policy(env: Env) -> (u32, u32, u32) {
        (
            MAX_DONATION_HISTORY,
            Self::effective_retention_cap(&env),
            PRUNE_MAX_BATCH,
        )
    }

    /// Set the admin-configurable retention cap (#986).
    ///
    /// Must be in `1..=MAX_DONATION_HISTORY`. This tunes how many records each
    /// list keeps **without** a contract upgrade, but can never remove the code
    /// ceiling that bounds storage growth.
    pub fn set_donation_retention_cap(env: Env, admin: Address, cap: u32) {
        admin.require_auth();
        Self::ensure_admin(&env, &admin);
        if cap == 0 || cap > MAX_DONATION_HISTORY {
            panic!("retention cap out of range");
        }
        env.storage().instance().set(&DataKey::RetentionCap, &cap);
        env.events().publish((Symbol::new(&env, "retcap"),), cap);
    }

    /// Remove a bounded batch of the oldest campaign donation records (#986).
    ///
    /// This follows `analytics::prune_earnings` (#651): `limit` is mandatory and
    /// capped at [`PRUNE_MAX_BATCH`] and bounds both the number of records
    /// removed and the number of records **examined**, the caller states the
    /// window with `upto_index` rather than "everything", the admin must
    /// authenticate, and the scan **stops** at the first record still inside
    /// [`MIN_RETENTION_SECONDS`] so a prune can never reach past protected data.
    ///
    /// It removes only the per-campaign **list** entry. `CampaignRaised` — the
    /// reconciliation total — is never touched, so the campaign's financial
    /// summary survives any prune.
    pub fn prune_campaign_donations(
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

        let mut donations: Vec<Donation> = env
            .storage()
            .persistent()
            .get(&DataKey::CampaignDonations(campaign_id))
            .unwrap_or(Vec::new(&env));
        // Donations are appended oldest-first, so the front of the list is the
        // oldest and the first in-window record marks the end of the range.
        let cutoff = env.ledger().timestamp().saturating_sub(MIN_RETENTION_SECONDS);
        let target = upto_index.min(donations.len()).min(limit);
        let mut removed = 0u32;
        while removed < target {
            let oldest = donations.get(0).unwrap();
            if oldest.timestamp > cutoff {
                break;
            }
            donations.pop_front();
            removed += 1;
        }

        env.storage().persistent().set(&DataKey::CampaignDonations(campaign_id), &donations);
        env.events().publish(
            (Symbol::new(&env, "prune"),),
            (campaign_id, removed, cutoff),
        );
        removed
    }

    pub fn upgrade(env: Env, admin: Address, new_wasm_hash: BytesN<32>) {
        admin.require_auth();
        Self::ensure_admin(&env, &admin);
        env.deployer().update_current_contract_wasm(&new_wasm_hash);
    }

    fn ensure_admin(env: &Env, admin: &Address) {
        let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if stored_admin != *admin {
            panic!("unauthorized");
        }
    }

    /// The retention cap actually applied to both lists (#986): the stored
    /// admin value (or [`DEFAULT_DONATION_HISTORY`]), clamped to
    /// [`MAX_DONATION_HISTORY`].
    fn effective_retention_cap(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::RetentionCap)
            .unwrap_or(DEFAULT_DONATION_HISTORY)
            .min(MAX_DONATION_HISTORY)
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
    use soroban_sdk::testutils::{Address as _, Ledger as _};
    use soroban_sdk::{contract, contractimpl, testutils::Events as _, token, Env};

    /// Minimal stand-in for the campaign contract the donation contract calls
    /// into on every donation.
    #[contract]
    pub struct MockCampaign;

    #[contractimpl]
    impl MockCampaign {
        pub fn init(env: Env, campaign: Campaign) {
            env.storage().instance().set(&0u32, &campaign);
        }
        pub fn update_raised(_env: Env, _campaign_id: u64, _amount: i128) {}
        pub fn record_donor(_env: Env, _campaign_id: u64, _donor: Address) {}
        pub fn record_donation(
            _env: Env,
            _campaign_id: u64,
            _donor: Address,
            amount: i128,
            _asset_code: String,
        ) -> i128 {
            amount
        }
        pub fn get_campaign(env: Env, _campaign_id: u64) -> Option<Campaign> {
            env.storage().instance().get(&0u32)
        }
    }

    const CAMPAIGN_ID: u64 = 7;

    struct Fixture<'a> {
        env: Env,
        client: DonationContractClient<'a>,
        token: Address,
        donor: Address,
        admin: Address,
    }

    fn setup<'a>() -> Fixture<'a> {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let donor = Address::generate(&env);
        let owner = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let token = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        token::StellarAssetClient::new(&env, &token).mint(&donor, &1_000_000);

        let campaign_id = env.register_contract(None, MockCampaign);
        MockCampaignClient::new(&env, &campaign_id).init(&Campaign {
            id: CAMPAIGN_ID,
            owner,
            goal: 0,
            raised: 0,
            status: CampaignStatus::Active,
            deadline: 0,
            fee_bps: 0,
            platform_wallet: None,
        });

        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);
        client.initialize(&admin, &campaign_id);

        Fixture {
            env,
            client,
            token,
            donor,
            admin,
        }
    }

    fn asset_code(env: &Env) -> String {
        String::from_str(env, "USDC")
    }

    #[test]
    fn donation_flow_records_history_and_total() {
        let f = setup();
        f.client.donate(
            &f.donor,
            &CAMPAIGN_ID,
            &100_i128,
            &f.token,
            &asset_code(&f.env),
            &false,
            &None,
        );

        let (donations, info) = f.client.get_donations_for_campaign(&CAMPAIGN_ID, &0, &10);
        assert_eq!(donations.len(), 1);
        assert_eq!(info.total, 1);
        assert!(!info.has_more);
        assert_eq!(f.client.get_total_raised(&CAMPAIGN_ID), 100_i128);

        let (history, _) = f.client.get_donor_history(&f.donor, &0, &10);
        assert_eq!(history.len(), 1);

        // The contract received the tokens.
        let contract_id = f.client.address.clone();
        assert_eq!(token::Client::new(&f.env, &f.token).balance(&contract_id), 100);
    }

    #[test]
    fn anonymous_donation_does_not_track_donor() {
        let f = setup();
        f.client.donate(
            &f.donor,
            &CAMPAIGN_ID,
            &100_i128,
            &f.token,
            &asset_code(&f.env),
            &true,
            &None,
        );

        let (history, _) = f.client.get_donor_history(&f.donor, &0, &10);
        assert_eq!(history.len(), 0);

        let (donations, _) = f.client.get_donations_for_campaign(&CAMPAIGN_ID, &0, &10);
        assert_eq!(donations.len(), 1);
    }

    /// The list is ring-trimmed to the retention cap on every write, and the
    /// page metadata says so (#986).
    #[test]
    fn retention_cap_ring_trims_the_list() {
        let f = setup();
        f.client.set_donation_retention_cap(&f.admin, &3);

        for _ in 0..5 {
            f.client.donate(
                &f.donor,
                &CAMPAIGN_ID,
                &10_i128,
                &f.token,
                &asset_code(&f.env),
                &false,
                &None,
            );
        }

        let (donations, info) = f.client.get_donations_for_campaign(&CAMPAIGN_ID, &0, &10);
        assert_eq!(donations.len(), 3, "cap must trim the retained window");
        assert_eq!(info.total, 3);
        assert_eq!(f.client.get_donation_retention_cap(), 3);
        // The aggregate is never trimmed.
        assert_eq!(f.client.get_total_raised(&CAMPAIGN_ID), 50_i128);
    }

    #[test]
    fn retention_cap_out_of_range_is_rejected() {
        let f = setup();
        let too_big = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.client
                .set_donation_retention_cap(&f.admin, &(MAX_DONATION_HISTORY + 1));
        }));
        assert!(too_big.is_err());
    }

    /// `prune_campaign_donations` removes only records older than the retention
    /// window, stops at the first protected record, and is bounded by `limit`.
    #[test]
    fn prune_is_bounded_and_respects_the_retention_window() {
        let f = setup();
        // Force a non-zero clock so the retention window is meaningful.
        f.env.ledger().set_timestamp(MIN_RETENTION_SECONDS + 1_000);
        for _ in 0..5 {
            f.client.donate(
                &f.donor,
                &CAMPAIGN_ID,
                &10_i128,
                &f.token,
                &asset_code(&f.env),
                &false,
                &None,
            );
        }

        // All five were stamped at `MIN_RETENTION_SECONDS + 1_000`; with the
        // clock moved forward, they are inside the window and protected.
        let removed_now = f
            .client
            .prune_campaign_donations(&f.admin, &CAMPAIGN_ID, &5, &5);
        assert_eq!(removed_now, 0, "in-window records must never be removed");

        // Move the clock past the window: the records become prunable.
        f.env
            .ledger()
            .set_timestamp(MIN_RETENTION_SECONDS * 2 + 10_000);
        let removed = f
            .client
            .prune_campaign_donations(&f.admin, &CAMPAIGN_ID, &5, &2);
        assert_eq!(removed, 2, "limit bounds one prune call");
    }

    /// `donate` publishes exactly two events: the contract's own
    /// `donation_made`, and the `transfer` event the Stellar Asset Contract
    /// emits for the token movement. Both are contract events, so both appear
    /// in `env.events().all()`; asserting the difference around the call pins
    /// them. A third event (a second `publish`, or a second token op) would
    /// fail here before it became a fee regression.
    #[test]
    fn donation_event_budget_is_two() {
        let f = setup();
        let before = f.env.events().all().len();
        f.client.donate(
            &f.donor,
            &CAMPAIGN_ID,
            &100_i128,
            &f.token,
            &asset_code(&f.env),
            &false,
            &None,
        );
        assert_eq!((f.env.events().all().len() - before) as u32, 2);
    }

    #[test]
    fn pause_blocks_donations() {
        let f = setup();
        f.client.pause(&f.admin);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.client.donate(
                &f.donor,
                &CAMPAIGN_ID,
                &100_i128,
                &f.token,
                &asset_code(&f.env),
                &false,
                &None,
            );
        }));
        assert!(result.is_err());

        f.client.unpause(&f.admin);
        f.client.donate(
            &f.donor,
            &CAMPAIGN_ID,
            &100_i128,
            &f.token,
            &asset_code(&f.env),
            &false,
            &None,
        );
        assert_eq!(f.client.get_total_raised(&CAMPAIGN_ID), 100_i128);
    }

    #[test]
    fn donate_with_nonce_rejects_duplicate() {
        let f = setup();
        f.client.donate_with_nonce(
            &f.donor,
            &CAMPAIGN_ID,
            &100_i128,
            &f.token,
            &asset_code(&f.env),
            &false,
            &None,
            &42_u64,
        );
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.client.donate_with_nonce(
                &f.donor,
                &CAMPAIGN_ID,
                &100_i128,
                &f.token,
                &asset_code(&f.env),
                &false,
                &None,
                &42_u64,
            );
        }));
        assert!(result.is_err(), "a reused nonce must be rejected");
    }

    #[test]
    fn refund_only_for_rejected_campaign() {
        let f = setup();
        // The mock campaign is `Active`, so a refund is refused.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.client
                .refund(&f.admin, &CAMPAIGN_ID, &f.donor, &100_i128, &f.token);
        }));
        assert!(result.is_err());
    }
}
