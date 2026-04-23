#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, vec, Address, Env, String, Symbol, Vec,
};

// ── Constants ────────────────────────────────────────────────────────────────

/// Issue #103 – Stellar base fee in stroops (1 XLM = 10_000_000 stroops)
const BASE_FEE: i128 = 100;

// ── Storage key helpers ──────────────────────────────────────────────────────

fn campaign_key(id: u64) -> (Symbol, u64) {
    (symbol_short!("camp"), id)
}

/// Issue #102 – per-campaign per-asset raised total key
fn asset_raised_key(campaign_id: u64, asset: &Symbol) -> (Symbol, u64, Symbol) {
    (symbol_short!("araised"), campaign_id, asset.clone())
}

/// Issue #104 – ordered donation record list key
fn history_key(campaign_id: u64) -> (Symbol, u64) {
    (symbol_short!("history"), campaign_id)
}

// ── Data types ───────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Campaign {
    pub id: u64,
    pub creator: Address,
    pub title: Symbol,
    pub goal: i128,
    pub raised: i128,
    pub deadline: u64,
    pub active: bool,
}

/// Issue #104 – one entry in the donation history list
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DonationRecord {
    pub donor: Address,
    pub amount: i128,   // net amount after fee
    pub fee: i128,
    pub asset: Symbol,
    pub timestamp: u64,
}

// ── Contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct StellarAidContract;

#[contractimpl]
impl StellarAidContract {
    /// Initialize the contract with admin address
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        env.storage().instance().set(&symbol_short!("admin"), &admin);
        env.storage().instance().set(&symbol_short!("count"), &0u64);
    }

    /// Ping method for health check
    pub fn ping() -> u32 {
        1
    }

    /// Create a new campaign
    pub fn create_campaign(
        env: Env,
        creator: Address,
        title: Symbol,
        goal: i128,
        deadline: u64,
    ) -> u64 {
        creator.require_auth();

        let mut count: u64 = env
            .storage()
            .instance()
            .get(&symbol_short!("count"))
            .unwrap_or(0);

        count += 1;

        let campaign = Campaign {
            id: count,
            creator: creator.clone(),
            title,
            goal,
            raised: 0,
            deadline,
            active: true,
        };

        env.storage().persistent().set(&campaign_key(count), &campaign);
        env.storage().instance().set(&symbol_short!("count"), &count);

        count
    }

    /// Donate to a campaign.
    ///
    /// Issue #102 – accepts an `asset` parameter (e.g. symbol_short!("XLM")),
    ///              validates it is non-empty, tracks per-asset raised totals.
    /// Issue #103 – deducts BASE_FEE (100 stroops) from `amount` before
    ///              crediting the campaign; panics if amount <= fee.
    /// Issue #104 – appends a DonationRecord to the campaign's history list.
    pub fn donate(env: Env, donor: Address, campaign_id: u64, amount: i128, asset: Symbol) {
        donor.require_auth();

        // Issue #102 – validate asset is provided (Symbol must not be the empty/default value)
        assert!(asset != Symbol::new(&env, ""), "Asset must be specified");
        assert!(amount > BASE_FEE, "Amount must exceed the base fee");

        let mut campaign: Campaign = env
            .storage()
            .persistent()
            .get(&campaign_key(campaign_id))
            .expect("Campaign not found");

        assert!(campaign.active, "Campaign is not active");

        // Issue #103 – calculate and deduct fee
        let fee = BASE_FEE;
        let net = amount - fee;

        campaign.raised += net;
        env.storage().persistent().set(&campaign_key(campaign_id), &campaign);

        // Issue #102 – update per-asset raised total
        let prev_asset_raised: i128 = env
            .storage()
            .persistent()
            .get(&asset_raised_key(campaign_id, &asset))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&asset_raised_key(campaign_id, &asset), &(prev_asset_raised + net));

        // Issue #104 – append to donation history
        let record = DonationRecord {
            donor: donor.clone(),
            amount: net,
            fee,
            asset,
            timestamp: env.ledger().timestamp(),
        };
        let mut history: Vec<DonationRecord> = env
            .storage()
            .persistent()
            .get(&history_key(campaign_id))
            .unwrap_or_else(|| vec![&env]);
        history.push_back(record);
        env.storage().persistent().set(&history_key(campaign_id), &history);
    }

    /// Get campaign details
    pub fn get_campaign(env: Env, campaign_id: u64) -> Option<Campaign> {
        env.storage().persistent().get(&campaign_key(campaign_id))
    }

    /// Issue #102 – get total raised for a specific asset on a campaign
    pub fn get_asset_raised(env: Env, campaign_id: u64, asset: Symbol) -> i128 {
        env.storage()
            .persistent()
            .get(&asset_raised_key(campaign_id, &asset))
            .unwrap_or(0)
    }

    /// Issue #103 – expose the fee constant so callers can calculate upfront
    pub fn get_base_fee() -> i128 {
        BASE_FEE
    }

    /// Issue #104 – paginated donation history for a campaign.
    /// `page` is 0-indexed; returns up to `page_size` records.
    pub fn get_donation_history(
        env: Env,
        campaign_id: u64,
        page: u32,
        page_size: u32,
    ) -> Vec<DonationRecord> {
        let history: Vec<DonationRecord> = env
            .storage()
            .persistent()
            .get(&history_key(campaign_id))
            .unwrap_or_else(|| vec![&env]);

        let total = history.len();
        let start = page * page_size;
        if start >= total {
            return vec![&env];
        }

        let end = (start + page_size).min(total);
        let mut page_records: Vec<DonationRecord> = vec![&env];
        for i in start..end {
            page_records.push_back(history.get(i).unwrap());
        }
        page_records
    }

    /// Get admin address
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&symbol_short!("admin"))
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn test_ping() {
        let env = Env::default();
        let contract_id = env.register_contract(None, StellarAidContract);
        let client = StellarAidContractClient::new(&env, &contract_id);
        assert_eq!(client.ping(), 1);
    }

    #[test]
    fn test_initialize() {
        let env = Env::default();
        let contract_id = env.register_contract(None, StellarAidContract);
        let client = StellarAidContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        assert_eq!(client.get_admin(), Some(admin));
    }

    #[test]
    fn test_donate_multi_asset_fee_and_history() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, StellarAidContract);
        let client = StellarAidContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let creator = Address::generate(&env);
        let cid = client.create_campaign(&creator, &symbol_short!("test"), &10000, &9999999);

        let donor = Address::generate(&env);
        let xlm = symbol_short!("XLM");
        let usdc = symbol_short!("USDC");

        // #103 – fee deducted: net = 1000 - 100 = 900
        client.donate(&donor, &cid, &1000, &xlm);
        // #102 – different asset
        client.donate(&donor, &cid, &500, &usdc);

        // #103 – campaign.raised = 900 + 400 = 1300
        let campaign = client.get_campaign(&cid).unwrap();
        assert_eq!(campaign.raised, 1300);

        // #102 – per-asset totals
        assert_eq!(client.get_asset_raised(&cid, &xlm), 900);
        assert_eq!(client.get_asset_raised(&cid, &usdc), 400);

        // #103 – base fee constant
        assert_eq!(client.get_base_fee(), 100);

        // #104 – history has 2 records
        let page = client.get_donation_history(&cid, &0, &10);
        assert_eq!(page.len(), 2);

        // #104 – pagination: page_size=1 returns 1 record
        let p0 = client.get_donation_history(&cid, &0, &1);
        assert_eq!(p0.len(), 1);
        let p1 = client.get_donation_history(&cid, &1, &1);
        assert_eq!(p1.len(), 1);
    }
}
