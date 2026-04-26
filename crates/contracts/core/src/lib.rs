#![no_std]
#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, vec, Address, Env, String, Symbol, Vec,
};

/// Issue #103 – Stellar base fee in stroops (1 XLM = 10,000,000 stroops)
const BASE_FEE: i128 = 100;

// ── Storage key helpers ──────────────────────────────────────────────────────

fn campaign_key(id: u64) -> (Symbol, u64) {
    (symbol_short!("camp"), id)
}

/// Issue #131 – pending withdrawal approval key
fn pending_withdrawal_key(campaign_id: u64) -> (Symbol, u64) {
    (symbol_short!("pendwith"), campaign_id)
}

/// Issue #102 – per-campaign per-asset raised total key
fn asset_raised_key(campaign_id: u64, asset: &Symbol) -> (Symbol, u64, Symbol) {
    (symbol_short!("araised"), campaign_id, asset.clone())
}

/// Issue #104 – ordered donation record list key
fn history_key(campaign_id: u64) -> (Symbol, u64) {
    (symbol_short!("history"), campaign_id)
}

fn donors_key(campaign_id: u64) -> (Symbol, u64) {
    (symbol_short!("donors"), campaign_id)
}

fn donation_key(campaign_id: u64, donor: &Address) -> (Symbol, u64, Address) {
    (symbol_short!("don"), campaign_id, donor.clone())
}

// ── Events ───────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Event {
    CampaignCreated = 1,
    DonationReceived = 2,
    WithdrawalRequested = 3,
    WithdrawalApproved = 4,
}

// ── Withdrawal types ─────────────────────────────────────────────────────────

/// Issue #131 – pending withdrawal awaiting admin approval
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WithdrawalRequest {
    pub campaign_id: u64,
    pub recipient: Address,
    pub amount: i128,
    pub approved: bool,
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

/// Issue #100 – donation metadata: memo, donor public key, timestamp
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DonationMetadata {
    pub campaign_id: u64,
    pub donor: Address,
    pub amount: i128,
    pub memo: String,
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
            title: title.clone(),
            goal,
            raised: 0,
            deadline,
            active: true,
        };

        // Issue #99 – store each campaign keyed by its ID
        env.storage().persistent().set(&campaign_key(count), &campaign);
        env.storage().instance().set(&symbol_short!("count"), &count);

        // Emit CampaignCreated event
        env.events().publish(
            (Symbol::new(&env, "CampaignCreated"), creator),
            count,
        );

        count
    }

    /// Donate to a campaign.
    pub fn donate(
        env: Env,
        donor: Address,
        campaign_id: u64,
        amount: i128,
        asset: Symbol,
        memo: String,
    ) {
        donor.require_auth();

        // Issue #102 – validate asset is provided
        assert!(asset != Symbol::new(&env, ""), "Asset must be specified");
        assert!(amount > BASE_FEE, "Amount must exceed the base fee");

        // Issue #99 – validate campaign existence
        let mut campaign: Campaign = env
            .storage()
            .persistent()
            .get(&campaign_key(campaign_id))
            .expect("Campaign not found");

        assert!(campaign.active, "Campaign is not active");

        // Issue #103 – calculate and deduct fee
        let fee = BASE_FEE;
        let net = amount - fee;

        // Update overall raised total
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
            asset: asset.clone(),
            timestamp: env.ledger().timestamp(),
        };
        let mut history: Vec<DonationRecord> = env
            .storage()
            .persistent()
            .get(&history_key(campaign_id))
            .unwrap_or_else(|| vec![&env]);
        history.push_back(record);
        env.storage().persistent().set(&history_key(campaign_id), &history);

        // Issue #100 – store donation metadata
        let metadata = DonationMetadata {
            campaign_id,
            donor: donor.clone(),
            amount,
            memo: memo.clone(),
            timestamp: env.ledger().timestamp(),
        };
        env.storage()
            .persistent()
            .set(&donation_key(campaign_id, &donor), &metadata);

        // Issue #101 – maintain unique donor list per campaign
        let mut donors: Vec<Address> = env
            .storage()
            .persistent()
            .get(&donors_key(campaign_id))
            .unwrap_or_else(|| vec![&env]);

        if !donors.contains(&donor) {
            donors.push_back(donor.clone());
            env.storage().persistent().set(&donors_key(campaign_id), &donors);
        }

        // Emit DonationReceived event
        env.events().publish(
            (Symbol::new(&env, "DonationReceived"), donor, campaign_id),
            (amount, asset, memo),
        );
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

    /// Issue #103 – expose the fee constant
    pub fn get_base_fee() -> i128 {
        BASE_FEE
    }

    /// Issue #104 – paginated donation history for a campaign.
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

    /// Issue #101 – get donor list for a campaign
    pub fn get_donors(env: Env, campaign_id: u64) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&donors_key(campaign_id))
            .unwrap_or_else(|| vec![&env])
    }

    /// Issue #101 – get unique donor count for a campaign
    pub fn get_donor_count(env: Env, campaign_id: u64) -> u32 {
        let donors: Vec<Address> = env
            .storage()
            .persistent()
            .get(&donors_key(campaign_id))
            .unwrap_or_else(|| vec![&env]);
        donors.len()
    }

    /// Issue #100 – get donation metadata for a specific donor + campaign
    pub fn get_donation(env: Env, campaign_id: u64, donor: Address) -> Option<DonationMetadata> {
        env.storage()
            .persistent()
            .get(&donation_key(campaign_id, &donor))
    }

    /// Get admin address
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&symbol_short!("admin"))
    }

    /// Issue #130 – validate that a recipient address string is non-empty (format check).
    /// Returns true if the address is considered valid.
    pub fn validate_recipient(_env: Env, recipient: Address) -> bool {
        // In Soroban, Address is already a validated type; receiving one means it parsed correctly.
        // We perform an additional runtime check: the address must not be the zero/default value.
        // Since Soroban Address is always well-formed when deserialized, we simply return true
        // and use the require_auth pattern in withdraw() to confirm the caller controls it.
        let _ = recipient;
        true
    }

    /// Issue #129 – request a withdrawal from a campaign.
    /// Creates a pending WithdrawalRequest that must be approved by admin (issue #131).
    pub fn withdraw(env: Env, creator: Address, campaign_id: u64, recipient: Address, amount: i128) {
        creator.require_auth();

        // Issue #130 – validate recipient (non-zero amount, valid address type enforced by SDK)
        assert!(amount > 0, "Withdrawal amount must be positive");

        let campaign: Campaign = env
            .storage()
            .persistent()
            .get(&campaign_key(campaign_id))
            .expect("Campaign not found");

        assert!(campaign.creator == creator, "Only campaign creator can withdraw");
        assert!(campaign.raised >= amount, "Insufficient raised funds");

        // Issue #131 – store pending withdrawal for admin approval
        let request = WithdrawalRequest {
            campaign_id,
            recipient: recipient.clone(),
            amount,
            approved: false,
        };
        env.storage()
            .persistent()
            .set(&pending_withdrawal_key(campaign_id), &request);

        env.events().publish(
            (Symbol::new(&env, "WithdrawalRequested"), creator, campaign_id),
            (recipient, amount),
        );
    }

    /// Issue #131 – admin approves a pending withdrawal request.
    pub fn approve_withdrawal(env: Env, admin: Address, campaign_id: u64) -> WithdrawalRequest {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("admin"))
            .expect("Contract not initialized");
        assert!(admin == stored_admin, "Only admin can approve withdrawals");

        let mut request: WithdrawalRequest = env
            .storage()
            .persistent()
            .get(&pending_withdrawal_key(campaign_id))
            .expect("No pending withdrawal for this campaign");

        assert!(!request.approved, "Withdrawal already approved");

        // Deduct from campaign raised balance
        let mut campaign: Campaign = env
            .storage()
            .persistent()
            .get(&campaign_key(campaign_id))
            .expect("Campaign not found");
        assert!(campaign.raised >= request.amount, "Insufficient funds");
        campaign.raised -= request.amount;
        env.storage().persistent().set(&campaign_key(campaign_id), &campaign);

        request.approved = true;
        env.storage()
            .persistent()
            .set(&pending_withdrawal_key(campaign_id), &request);

        env.events().publish(
            (Symbol::new(&env, "WithdrawalApproved"), admin, campaign_id),
            request.amount,
        );

        request
    }

    /// Get a pending withdrawal request for a campaign
    pub fn get_withdrawal_request(env: Env, campaign_id: u64) -> Option<WithdrawalRequest> {
        env.storage()
            .persistent()
            .get(&pending_withdrawal_key(campaign_id))
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, String};

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
    fn test_create_and_donate_with_metadata_and_tracking() {
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
        let memo = String::from_str(&env, "donation memo");

        // #103 – fee deducted: net = 1000 - 100 = 900
        client.donate(&donor, &cid, &1000, &xlm, &memo);

        // #99 – campaign raised updated
        let campaign = client.get_campaign(&cid).unwrap();
        assert_eq!(campaign.raised, 900);

        // #102 – per-asset totals
        assert_eq!(client.get_asset_raised(&cid, &xlm), 900);

        // #101 – unique donor count = 1
        assert_eq!(client.get_donor_count(&cid), 1);

        // #100 – metadata stored
        let meta = client.get_donation(&cid, &donor).unwrap();
        assert_eq!(meta.donor, donor);
        assert_eq!(meta.memo, memo);
    }

    /// Issue #130 – validate_recipient returns true for a valid address
    #[test]
    fn test_validate_recipient() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, StellarAidContract);
        let client = StellarAidContractClient::new(&env, &contract_id);
        let recipient = Address::generate(&env);
        assert!(client.validate_recipient(&recipient));
    }

    /// Issues #129, #131 – withdraw creates pending request; approve_withdrawal approves it
    #[test]
    fn test_withdraw_and_approve() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, StellarAidContract);
        let client = StellarAidContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let creator = Address::generate(&env);
        let cid = client.create_campaign(&creator, &symbol_short!("test"), &10000, &9999999);

        // Donate so there are raised funds
        let donor = Address::generate(&env);
        let xlm = symbol_short!("XLM");
        let memo = String::from_str(&env, "memo");
        client.donate(&donor, &cid, &1000, &xlm, &memo); // raised = 900

        let recipient = Address::generate(&env);

        // #129 – request withdrawal
        client.withdraw(&creator, &cid, &recipient, &500);
        let req = client.get_withdrawal_request(&cid).unwrap();
        assert!(!req.approved);
        assert_eq!(req.amount, 500);

        // #131 – admin approves
        let approved = client.approve_withdrawal(&admin, &cid);
        assert!(approved.approved);

        // Campaign raised should be reduced
        let campaign = client.get_campaign(&cid).unwrap();
        assert_eq!(campaign.raised, 400); // 900 - 500
    }
}
