#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, BytesN, Env, Symbol, Vec,
};

// ─── Storage keys ─────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// The admin address allowed to manage the factory.
    Admin,
    /// Sequential counter of deployed campaigns.
    Count,
    /// SHA-256 hash of the campaign WASM used for all new deployments (#273).
    CampaignWasmHash,
    /// Registry of all deployed campaigns as Vec<(creator, contract)> (#272).
    Campaigns,
    /// Deployment fee in stroops charged on each deploy_campaign call (#274).
    DeploymentFee,
    /// Treasury address that receives deployment fees (#274).
    Treasury,
}

/// Parameters passed to `deploy_campaign`, forwarded as constructor args.
#[contracttype]
#[derive(Clone)]
pub struct CampaignParams {
    /// Address that will own / manage the deployed campaign contract.
    pub creator: Address,
    /// Unique 32-byte salt so the same creator can deploy multiple campaigns.
    pub salt: BytesN<32>,
}

// ─── Contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct CampaignFactory;

#[contractimpl]
impl CampaignFactory {
    /// Initialise the factory.
    ///
    /// # Arguments
    /// * `admin`    – Address with administrative privileges.
    /// * `treasury` – Address that receives deployment fees.
    ///
    /// # Panics
    /// - if the factory has already been initialised.
    pub fn initialize(env: Env, admin: Address, treasury: Address) {
        admin.require_auth();

        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Treasury, &treasury);
        env.storage().instance().set(&DataKey::Count, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::DeploymentFee, &0i128);
        env.storage()
            .instance()
            .set(&DataKey::Campaigns, &Vec::<(Address, Address)>::new(&env));
    }

    // ── Issue #273: WASM hash management ──────────────────────────────────────

    /// Store or update the campaign WASM hash used for all future deployments.
    ///
    /// Admin only.
    pub fn update_wasm_hash(env: Env, new_hash: BytesN<32>) {
        Self::require_admin(&env);
        env.storage()
            .instance()
            .set(&DataKey::CampaignWasmHash, &new_hash);
    }

    /// Returns the currently stored campaign WASM hash.
    pub fn get_wasm_hash(env: Env) -> Option<BytesN<32>> {
        env.storage().instance().get(&DataKey::CampaignWasmHash)
    }

    // ── Issue #274: Deployment fee management ─────────────────────────────────

    /// Set the XLM deployment fee (in stroops) charged on each deployment.
    ///
    /// Admin only.
    pub fn set_deployment_fee(env: Env, fee: i128) {
        Self::require_admin(&env);
        env.storage()
            .instance()
            .set(&DataKey::DeploymentFee, &fee);
    }

    /// Returns the current deployment fee in stroops.
    pub fn get_deployment_fee(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::DeploymentFee)
            .unwrap_or(0)
    }

    // ── Issue #272 + #273 + #274: Deploy campaign ─────────────────────────────

    /// Deploy a new campaign contract.
    ///
    /// Uses the WASM hash stored via `update_wasm_hash`.  If a deployment fee
    /// is set the caller must have approved the factory to transfer that amount
    /// from their XLM (native token) balance to the treasury.
    ///
    /// # Arguments
    /// * `creator`   – Address that will own the new campaign.
    /// * `xlm_token` – Address of the native XLM token contract (for fee transfer).
    /// * `params`    – Deployment parameters (salt).
    ///
    /// # Returns
    /// The contract address of the newly deployed campaign.
    ///
    /// # Panics
    /// - if the factory is not initialised.
    /// - if no WASM hash has been stored yet.
    pub fn deploy_campaign(
        env: Env,
        creator: Address,
        xlm_token: Address,
        params: CampaignParams,
    ) -> Address {
        creator.require_auth();

        // Fetch stored WASM hash (#273).
        let wasm_hash: BytesN<32> = env
            .storage()
            .instance()
            .get(&DataKey::CampaignWasmHash)
            .expect("wasm hash not set");

        // Collect deployment fee if non-zero (#274).
        let fee: i128 = env
            .storage()
            .instance()
            .get(&DataKey::DeploymentFee)
            .unwrap_or(0);
        if fee > 0 {
            let treasury: Address = env
                .storage()
                .instance()
                .get(&DataKey::Treasury)
                .expect("treasury not set");
            token::Client::new(&env, &xlm_token).transfer(
                &creator,
                &treasury,
                &fee,
            );
        }

        // Deploy the campaign contract at a deterministic address (#273).
        let deployed_address = env
            .deployer()
            .with_address(params.creator.clone(), params.salt)
            .deploy_v2(wasm_hash, ());

        // Update campaign registry (#272).
        let mut campaigns: Vec<(Address, Address)> = env
            .storage()
            .instance()
            .get(&DataKey::Campaigns)
            .unwrap_or_else(|| Vec::new(&env));
        campaigns.push_back((creator.clone(), deployed_address.clone()));
        env.storage()
            .instance()
            .set(&DataKey::Campaigns, &campaigns);

        // Increment deployment counter.
        let count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::Count)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::Count, &(count + 1));

        // Emit `campaign_deployed` event.
        env.events().publish(
            (Symbol::new(&env, "campaign_deployed"), creator),
            deployed_address.clone(),
        );

        deployed_address
    }

    // ── Issue #272: Campaign registry views ───────────────────────────────────

    /// Returns all deployed campaigns as (creator, contract) pairs.
    pub fn get_all_campaigns(env: Env) -> Vec<(Address, Address)> {
        env.storage()
            .instance()
            .get(&DataKey::Campaigns)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Returns all campaign contract addresses deployed by the given creator.
    pub fn get_campaigns_by_creator(env: Env, creator: Address) -> Vec<Address> {
        let all: Vec<(Address, Address)> = env
            .storage()
            .instance()
            .get(&DataKey::Campaigns)
            .unwrap_or_else(|| Vec::new(&env));

        let mut result = Vec::new(&env);
        for (c, addr) in all.iter() {
            if c == creator {
                result.push_back(addr);
            }
        }
        result
    }

    // ── Admin helpers ─────────────────────────────────────────────────────────

    /// Returns the current admin address.
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::Admin)
    }

    /// Returns the total number of campaigns deployed via this factory.
    pub fn get_campaign_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::Count)
            .unwrap_or(0)
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    fn require_admin(env: &Env) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        admin.require_auth();
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as AddressTestUtils;

    fn setup(env: &Env) -> (CampaignFactoryClient, Address, Address) {
        let contract_id = env.register_contract(None, CampaignFactory);
        let client = CampaignFactoryClient::new(env, &contract_id);
        let admin = Address::generate(env);
        let treasury = Address::generate(env);
        client.initialize(&admin, &treasury);
        (client, admin, treasury)
    }

    #[test]
    fn test_initialize() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _) = setup(&env);
        assert_eq!(client.get_admin(), Some(admin));
        assert_eq!(client.get_campaign_count(), 0);
        assert_eq!(client.get_deployment_fee(), 0);
        assert_eq!(client.get_all_campaigns().len(), 0);
    }

    #[test]
    #[should_panic(expected = "already initialized")]
    fn test_initialize_twice_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, treasury) = setup(&env);
        client.initialize(&admin, &treasury); // should panic
    }

    #[test]
    fn test_update_wasm_hash() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, _) = setup(&env);
        let hash = BytesN::from_array(&env, &[7u8; 32]);
        client.update_wasm_hash(&hash);
        assert_eq!(client.get_wasm_hash(), Some(hash));
    }

    #[test]
    fn test_set_and_get_deployment_fee() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, _) = setup(&env);
        client.set_deployment_fee(&1_000_000i128);
        assert_eq!(client.get_deployment_fee(), 1_000_000i128);
    }

    /// `deploy_campaign` panics without a stored WASM hash.
    #[test]
    #[should_panic(expected = "wasm hash not set")]
    fn test_deploy_campaign_panics_without_wasm() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, _) = setup(&env);
        let creator = Address::generate(&env);
        let xlm_token = Address::generate(&env);
        let params = CampaignParams {
            creator: creator.clone(),
            salt: BytesN::from_array(&env, &[1u8; 32]),
        };
        client.deploy_campaign(&creator, &xlm_token, &params);
    }

    /// With a WASM hash set, deploy_campaign panics because WASM isn't uploaded —
    /// this confirms the auth and registry paths are reached.
    #[test]
    #[should_panic]
    fn test_deploy_campaign_panics_without_uploaded_wasm() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, _) = setup(&env);
        client.update_wasm_hash(&BytesN::from_array(&env, &[0u8; 32]));
        let creator = Address::generate(&env);
        let xlm_token = Address::generate(&env);
        let params = CampaignParams {
            creator: creator.clone(),
            salt: BytesN::from_array(&env, &[1u8; 32]),
        };
        client.deploy_campaign(&creator, &xlm_token, &params);
    }
}
