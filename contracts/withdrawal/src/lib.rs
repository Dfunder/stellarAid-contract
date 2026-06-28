#![no_std]

use soroban_sdk::{contract, contractclient, contractimpl, contracttype, Address, BytesN, Env, String, Symbol, Vec};
use shared::pause;
use shared::types::Withdrawal;

#[contractclient(name = "DonationContractClient")]
trait DonationContractTrait {
    fn get_total_raised(env: Env, campaign_id: u64) -> i128;
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Withdrawal(u64) = 0,
    WithdrawalsByCampaign(u64) = 1,
    Admin = 2,
    Initialized = 3,
    DonationContract = 3,
    WithdrawnAmount(u64) = 4,
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
    }

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
        let mut withdrawals = env.storage().persistent().get(&DataKey::WithdrawalsByCampaign(campaign_id)).unwrap_or(Vec::new(&env));
        withdrawals.push_back(withdrawal.clone());
        env.storage().persistent().set(&DataKey::WithdrawalsByCampaign(campaign_id), &withdrawals);
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
    pub fn approve_withdrawal(env: Env, withdrawal_id: u64, admin: Address) {
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

        let mut updated = withdrawal.clone();
        updated.approved = true;
        env.storage().persistent().set(&DataKey::Withdrawal(withdrawal_id), &updated);

        env.storage().persistent().set(&DataKey::WithdrawnAmount(campaign_id), &(already_withdrawn + withdrawal.amount));

        let tx_hash = BytesN::from_array(&env, &[0u8; 32]);
        env.events().publish((Symbol::new(&env, "withdrawal_approved"),), WithdrawalApprovedEvent { withdrawal_id, tx_hash });
    }

    /// Reject a withdrawal request with a reason.
    pub fn reject_withdrawal(env: Env, withdrawal_id: u64, admin: Address, reason: String) {
        pause::require_not_paused(&env);
        admin.require_auth();
        Self::ensure_admin(&env, &admin);
        let withdrawal = env.storage().persistent().get::<DataKey, Withdrawal>(&DataKey::Withdrawal(withdrawal_id)).unwrap();
        let _ = withdrawal;
        env.events().publish((Symbol::new(&env, "withdrawal_rejected"),), WithdrawalRejectedEvent { withdrawal_id, reason });
    }

    /// Get a withdrawal request by ID.
    pub fn get_withdrawal(env: Env, withdrawal_id: u64) -> Option<Withdrawal> {
        env.storage().persistent().get(&DataKey::Withdrawal(withdrawal_id))
    }

    /// Get all withdrawal requests for a given campaign.
    pub fn get_withdrawals_by_campaign(env: Env, campaign_id: u64) -> Vec<Withdrawal> {
        env.storage().persistent().get(&DataKey::WithdrawalsByCampaign(campaign_id)).unwrap_or(Vec::new(&env))
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

    fn next_withdrawal_id(env: &Env) -> u64 {
        let mut next_id: u64 = env.storage().instance().get(&Symbol::new(env, "next_withdrawal_id")).unwrap_or(1);
        env.storage().instance().set(&Symbol::new(env, "next_withdrawal_id"), &(next_id + 1));
        next_id
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    #[test]
    fn withdrawal_requests_and_approval_flow() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, WithdrawalContract);
        let client = WithdrawalContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let owner = Address::generate(&env);
        let recipient = Address::generate(&env);
        let donation_contract = Address::generate(&env);

        client.initialize(&admin, &donation_contract);
        let withdrawal_id = client.request_withdrawal(&7_u64, &owner, &120_i128, &recipient);

        let withdrawal = client.get_withdrawal(&withdrawal_id).unwrap();
        assert_eq!(withdrawal.amount, 120_i128);
        assert!(!withdrawal.approved);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.approve_withdrawal(&withdrawal_id, &admin);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn pause_blocks_withdrawal_requests() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, WithdrawalContract);
        let client = WithdrawalContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let owner = Address::generate(&env);
        let recipient = Address::generate(&env);
        let donation_contract = Address::generate(&env);

        client.initialize(&admin, &donation_contract);
        client.pause(&admin);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.request_withdrawal(&7_u64, &owner, &120_i128, &recipient);
        }));
        assert!(result.is_err());

        client.unpause(&admin);
        let id = client.request_withdrawal(&7_u64, &owner, &120_i128, &recipient);
        assert_eq!(id, 1);
    }
}
