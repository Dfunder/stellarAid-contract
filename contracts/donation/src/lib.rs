#![no_std]

use soroban_sdk::{contract, contractclient, contractimpl, contracttype, Address, BytesN, Env, String, Symbol, Vec};
use shared::types::{Campaign, CampaignStatus, Donation, DonationRefundedEvent, AnonymousDonationEvent};
use shared::pause;

#[contractclient(name = "CampaignContractClient")]
trait CampaignContractTrait {
    fn update_raised(env: Env, campaign_id: u64, amount: i128);
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
}

#[contracttype]
#[derive(Clone)]
pub struct DonationMadeEvent {
    pub donor: Address,
    pub campaign_id: u64,
    pub amount: i128,
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
    }

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

    pub fn donate(
        env: Env,
        donor: Address,
        campaign_id: u64,
        amount: i128,
        token_address: Option<Address>,
        anonymous: bool,
        memo: Option<String>,
    ) {
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

        let effective_donor = if anonymous {
            Address::generate(&env)
        } else {
            donor.clone()
        };

        let timestamp = env.ledger().timestamp();
        let donation = Donation {
            donor: effective_donor.clone(),
            campaign_id,
            amount,
            timestamp,
            memo: memo.clone(),
            anonymous,
            token_address: token_address.clone(),
        };

        let mut donations = env.storage().persistent().get(&DataKey::CampaignDonations(campaign_id)).unwrap_or(Vec::new(&env));
        donations.push_back(donation.clone());
        env.storage().persistent().set(&DataKey::CampaignDonations(campaign_id), &donations);

        if !anonymous {
            let mut history = env.storage().persistent().get(&DataKey::DonationHistory(donor.clone())).unwrap_or(Vec::new(&env));
            history.push_back(donation.clone());
            env.storage().persistent().set(&DataKey::DonationHistory(donor), &history);
        }

        let total = env.storage().persistent().get(&DataKey::CampaignRaised(campaign_id)).unwrap_or(0_i128);
        env.storage().persistent().set(&DataKey::CampaignRaised(campaign_id), &(total + amount));

        campaign_client.update_raised(&campaign_id, &amount);

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

    pub fn refund(env: Env, caller: Address, campaign_id: u64, donor: Address, amount: i128) {
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
        env.storage().persistent().set(&DataKey::CampaignRaised(campaign_id), &(total - amount));

        let mut donations: Vec<Donation> = env.storage().persistent().get(&DataKey::CampaignDonations(campaign_id)).unwrap_or(Vec::new(&env));
        let filtered: Vec<Donation> = donations.iter().filter(|d| d.donor != donor || d.amount != amount).collect();
        env.storage().persistent().set(&DataKey::CampaignDonations(campaign_id), &filtered);

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

    pub fn get_donations_for_campaign(env: Env, campaign_id: u64) -> Vec<Donation> {
        env.storage().persistent().get(&DataKey::CampaignDonations(campaign_id)).unwrap_or(Vec::new(&env))
    }

    pub fn get_total_raised(env: Env, campaign_id: u64) -> i128 {
        env.storage().persistent().get(&DataKey::CampaignRaised(campaign_id)).unwrap_or(0_i128)
    }

    pub fn get_donor_history(env: Env, donor: Address) -> Vec<Donation> {
        env.storage().persistent().get(&DataKey::DonationHistory(donor)).unwrap_or(Vec::new(&env))
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
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    #[test]
    fn donation_flow_records_history_and_total() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);
        let donor = Address::generate(&env);
        let admin = Address::generate(&env);
        let campaign_contract = Address::generate(&env);

        client.initialize(&admin, &campaign_contract);
        client.donate(&donor, &7_u64, &100_i128, &None, &false, &None);

        let donations = client.get_donations_for_campaign(&7_u64);
        assert_eq!(donations.len(), 1);
        assert_eq!(client.get_total_raised(&7_u64), 100_i128);
    }

    #[test]
    fn pause_blocks_donations() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);
        let donor = Address::generate(&env);
        let admin = Address::generate(&env);
        let campaign_contract = Address::generate(&env);

        client.initialize(&admin, &campaign_contract);
        client.pause(&admin);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.donate(&donor, &7_u64, &100_i128, &None, &false, &None);
        }));
        assert!(result.is_err());

        client.unpause(&admin);
        client.donate(&donor, &7_u64, &100_i128, &None, &false, &None);
        assert_eq!(client.get_total_raised(&7_u64), 100_i128);
    }

    #[test]
    fn anonymous_donation_does_not_track_donor() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);
        let donor = Address::generate(&env);
        let admin = Address::generate(&env);
        let campaign_contract = Address::generate(&env);

        client.initialize(&admin, &campaign_contract);
        client.donate(&donor, &7_u64, &100_i128, &None, &true, &None);

        let history = client.get_donor_history(&donor);
        assert_eq!(history.len(), 0);

        let donations = client.get_donations_for_campaign(&7_u64);
        assert_eq!(donations.len(), 1);
    }

    #[test]
    fn refund_only_for_rejected_campaign() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);
        let donor = Address::generate(&env);
        let admin = Address::generate(&env);
        let campaign_contract = Address::generate(&env);

        client.initialize(&admin, &campaign_contract);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.refund(&admin, &7_u64, &donor, &100_i128);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn donation_with_token_address() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);
        let donor = Address::generate(&env);
        let admin = Address::generate(&env);
        let campaign_contract = Address::generate(&env);
        let token = Address::generate(&env);

        client.initialize(&admin, &campaign_contract);
        client.donate(&donor, &7_u64, &100_i128, &Some(token), &false, &None);

        let donations = client.get_donations_for_campaign(&7_u64);
        assert_eq!(donations.len(), 1);
        assert_eq!(donations.get(0).unwrap().token_address, Some(token));
    }

    #[test]
    fn donation_with_memo() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);
        let donor = Address::generate(&env);
        let admin = Address::generate(&env);
        let campaign_contract = Address::generate(&env);
        let memo = String::from_str(&env, "Happy Birthday!");

        client.initialize(&admin, &campaign_contract);
        client.donate(&donor, &7_u64, &100_i128, &None, &false, &Some(memo.clone()));

        let donations = client.get_donations_for_campaign(&7_u64);
        assert_eq!(donations.get(0).unwrap().memo, Some(memo));
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    #[test]
    fn donation_flow_records_history_and_total() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);
        let donor = Address::generate(&env);
        let admin = Address::generate(&env);
        let campaign_contract = Address::generate(&env);

        client.initialize(&admin, &campaign_contract);
        client.donate(&donor, &7_u64, &100_i128);

        let donations = client.get_donations_for_campaign(&7_u64);
        assert_eq!(donations.len(), 1);
        assert_eq!(client.get_total_raised(&7_u64), 100_i128);

        let history = client.get_donor_history(&donor);
        assert_eq!(history.len(), 1);
        assert_eq!(history.get(0).unwrap().amount, 100_i128);
    }

    #[test]
    fn pause_blocks_donations() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);
        let donor = Address::generate(&env);
        let admin = Address::generate(&env);
        let campaign_contract = Address::generate(&env);

        client.initialize(&admin, &campaign_contract);
        client.pause(&admin);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.donate(&donor, &7_u64, &100_i128);
        }));
        assert!(result.is_err());

        client.unpause(&admin);
        client.donate(&donor, &7_u64, &100_i128);
        assert_eq!(client.get_total_raised(&7_u64), 100_i128);
    }
}
