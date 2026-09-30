#![no_std]

use campaign::{CampaignContract, CampaignContractClient};
use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Bytes, Env, Symbol, Vec};

include!("../../semver_types.rs");

#[contracttype]
pub enum DataKey {
    Admin,
    Initialized,
    Campaigns,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignDeployedEvent {
    pub creator: Address,
    pub contract: Address,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum CampaignFactoryError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
}

fn read_campaigns(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&DataKey::Campaigns)
        .unwrap_or_else(|| Vec::new(env))
}

#[contract]
pub struct CampaignFactory;

#[contractimpl]
impl CampaignFactory {
    pub fn initialize(env: Env, admin: Address) -> Result<(), CampaignFactoryError> {
        if env.storage().instance().has(&DataKey::Initialized) {
            return Err(CampaignFactoryError::AlreadyInitialized);
        }

        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage()
            .instance()
            .set(&DataKey::Campaigns, &Vec::<Address>::new(&env));
        Ok(())
    }

    impl_semver_queries!();

    pub fn deploy_campaign(
        env: Env,
        creator: Address,
        _params: Bytes,
    ) -> Result<Address, CampaignFactoryError> {
        if !env.storage().instance().has(&DataKey::Initialized) {
            return Err(CampaignFactoryError::NotInitialized);
        }

        creator.require_auth();

        let contract_id = env.register_contract(None, CampaignContract);
        let client = CampaignContractClient::new(&env, &contract_id);
        client.initialize(&creator);

        let mut campaigns = read_campaigns(&env);
        campaigns.push_back(contract_id.clone());
        env.storage().instance().set(&DataKey::Campaigns, &campaigns);

        env.events().publish(
            (Symbol::new(&env, "campaign_deployed"),),
            CampaignDeployedEvent {
                creator: creator.clone(),
                contract: contract_id.clone(),
            },
        );

        Ok(contract_id)
    }

    /// Every campaign deployed through this factory, in deployment order.
    ///
    /// Unbounded: `deploy_campaign` appends to the registry for the life of the
    /// factory. Prefer [`get_all_campaigns_page`](Self::get_all_campaigns_page),
    /// which bounds the response with `shared::pagination`. Kept for callers
    /// that already rely on the unbounded shape.
    pub fn get_all_campaigns(env: Env) -> Vec<Address> {
        read_campaigns(&env)
    }

    /// A bounded page of the campaign registry, plus its `PageInfo`.
    ///
    /// A page spans at most `shared::pagination::MAX_LIMIT` campaigns however
    /// large `limit` is. A `limit` of `0` yields the default page size rather
    /// than an empty result, and an `offset` past the end is an empty page
    /// rather than an error, so a client can page to the end without reading a
    /// count first.
    pub fn get_all_campaigns_page(
        env: Env,
        offset: u32,
        limit: u32,
    ) -> (Vec<Address>, shared::pagination::PageInfo) {
        let campaigns = read_campaigns(&env);
        shared::pagination::paginated(&env, &campaigns, offset, limit)
    }

    /// Every campaign in the registry whose admin is `creator`.
    ///
    /// Unbounded, and more expensive than it looks: this is one cross-contract
    /// `get_admin` call per registered campaign, so its cost grows with the
    /// whole registry rather than with the number of matches. Prefer
    /// [`get_campaigns_by_creator_page`](Self::get_campaigns_by_creator_page).
    pub fn get_campaigns_by_creator(env: Env, creator: Address) -> Vec<Address> {
        campaigns_by_creator(&env, &creator)
    }

    /// A bounded page of `creator`'s campaigns, plus its `PageInfo`.
    ///
    /// The page bounds the *response*, not the work: the registry is still
    /// scanned end to end and one `get_admin` call is still made per registered
    /// campaign, because the matches cannot be located without the scan. What
    /// this buys is a bounded response and an honest `PageInfo`, so a client can
    /// tell a truncated page from a complete answer. A future version should
    /// maintain a `DataKey::CampaignsByCreator(Address)` reverse index written
    /// in `deploy_campaign` — see the note on reverse indexes in
    /// `docs/QUERY_OPTIMIZATION.md` §4.
    pub fn get_campaigns_by_creator_page(
        env: Env,
        creator: Address,
        offset: u32,
        limit: u32,
    ) -> (Vec<Address>, shared::pagination::PageInfo) {
        let matched = campaigns_by_creator(&env, &creator);
        shared::pagination::paginated(&env, &matched, offset, limit)
    }
}

/// Campaigns in the registry whose admin is `creator`.
///
/// Kept out of the `#[contractimpl]` block: the two readers above call it, and
/// a plain function is not added to the contract spec.
fn campaigns_by_creator(env: &Env, creator: &Address) -> Vec<Address> {
    let campaigns = read_campaigns(env);
    let mut filtered = Vec::new(env);

    for campaign in campaigns.iter() {
        let contract_id = campaign;
        let client = CampaignContractClient::new(env, &contract_id);
        if client.get_admin() == *creator {
            filtered.push_back(contract_id);
        }
    }

    filtered
}
