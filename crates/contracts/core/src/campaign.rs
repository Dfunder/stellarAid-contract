use crate::assets::AssetResolver;
use crate::rbac::Rbac;
use soroban_sdk::{contracttype, Address, Env, String, Vec};

#[derive(Clone, Copy, Eq, PartialEq)]
#[contracttype]
pub enum CampaignStatus {
    Pending,
    Active,
    Completed,
    Cancelled,
    Expired,
}

#[derive(Clone, Eq, PartialEq)]
#[contracttype]
pub enum CampaignListFilter {
    All,
    Pending,
    Active,
    Completed,
    Cancelled,
    Expired,
}

#[derive(Clone)]
#[contracttype]
pub struct Campaign {
    pub project_id: String,
    pub title: String,
    pub description: String,
    pub beneficiary: Address,
    pub goal_amount: i128,
    pub goal_asset: String,
    pub start_timestamp: u64,
    pub end_timestamp: u64,
    pub category: String,
    pub tags: Vec<String>,
    pub status: CampaignStatus,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct CampaignStats {
    pub project_id: String,
    pub status: CampaignStatus,
    pub goal_amount: i128,
    pub raised_amount: i128,
    pub progress_percentage: u32,
    pub donor_count: u32,
    pub donation_count: u32,
    pub remaining_amount: i128,
}

#[derive(Clone)]
#[contracttype]
enum CampaignStorageKey {
    Campaign(String),
    CampaignIds,
    RaisedAmount(String),
    DonationCount(String),
    DonorCount(String),
    Donor(String, Address),
}

pub struct CampaignManager;

impl CampaignManager {
    pub fn create_campaign(
        env: &Env,
        caller: &Address,
        project_id: String,
        title: String,
        description: String,
        beneficiary: Address,
        goal_amount: i128,
        goal_asset: String,
        start_timestamp: u64,
        end_timestamp: u64,
        category: String,
        tags: Vec<String>,
    ) -> Result<Campaign, &'static str> {
        Rbac::require_admin_auth(env, caller);
        Self::validate_campaign_creation(
            env,
            &project_id,
            &title,
            &description,
            goal_amount,
            &goal_asset,
            start_timestamp,
            end_timestamp,
        )?;

        if Self::get_campaign(env, &project_id).is_some() {
            return Err("Campaign already exists");
        }

        let now = env.ledger().timestamp();
        let status = if now < start_timestamp {
            CampaignStatus::Pending
        } else {
            CampaignStatus::Active
        };

        let campaign = Campaign {
            project_id: project_id.clone(),
            title,
            description,
            beneficiary,
            goal_amount,
            goal_asset,
            start_timestamp,
            end_timestamp,
            category,
            tags,
            status,
            created_at: now,
            updated_at: now,
        };

        Self::save_campaign(env, &campaign);
        Self::append_campaign_id(env, &project_id);
        env.storage()
            .instance()
            .set(&CampaignStorageKey::RaisedAmount(project_id.clone()), &0i128);
        env.storage()
            .instance()
            .set(&CampaignStorageKey::DonationCount(project_id), &0u32);
        env.storage()
            .instance()
            .set(&CampaignStorageKey::DonorCount(campaign.project_id.clone()), &0u32);

        Ok(campaign)
    }

    pub fn get_campaign(env: &Env, project_id: &String) -> Option<Campaign> {
        let mut campaign: Campaign = env
            .storage()
            .instance()
            .get(&CampaignStorageKey::Campaign(project_id.clone()))?;
        let resolved_status = Self::resolve_status(&campaign, env.ledger().timestamp());
        if resolved_status != campaign.status {
            campaign.status = resolved_status;
            campaign.updated_at = env.ledger().timestamp();
            Self::save_campaign(env, &campaign);
        }
        Some(campaign)
    }

    pub fn update_campaign(
        env: &Env,
        caller: &Address,
        project_id: String,
        title: String,
        description: String,
        beneficiary: Address,
        category: String,
        tags: Vec<String>,
    ) -> Result<Campaign, &'static str> {
        Rbac::require_admin_auth(env, caller);

        if title.is_empty() {
            return Err("Title cannot be empty");
        }
        if description.is_empty() {
            return Err("Description cannot be empty");
        }

        let mut campaign = Self::get_campaign(env, &project_id).ok_or("Campaign not found")?;
        if campaign.status == CampaignStatus::Cancelled
            || campaign.status == CampaignStatus::Completed
            || campaign.status == CampaignStatus::Expired
        {
            return Err("Cannot update finalized campaign");
        }

        campaign.title = title;
        campaign.description = description;
        campaign.beneficiary = beneficiary;
        campaign.category = category;
        campaign.tags = tags;
        campaign.updated_at = env.ledger().timestamp();
        Self::save_campaign(env, &campaign);
        Ok(campaign)
    }

    pub fn complete_campaign(
        env: &Env,
        caller: &Address,
        project_id: String,
    ) -> Result<Campaign, &'static str> {
        Rbac::require_admin_auth(env, caller);
        Self::set_status(env, &project_id, CampaignStatus::Completed)
    }

    pub fn cancel_campaign(
        env: &Env,
        caller: &Address,
        project_id: String,
    ) -> Result<Campaign, &'static str> {
        Rbac::require_admin_auth(env, caller);
        Self::set_status(env, &project_id, CampaignStatus::Cancelled)
    }

    pub fn list_campaigns(env: &Env, filter: CampaignListFilter) -> Vec<Campaign> {
        let ids = Self::get_campaign_ids(env);
        let mut out = Vec::new(env);
        for project_id in ids.iter() {
            if let Some(campaign) = Self::get_campaign(env, &project_id) {
                if Self::matches_filter(&campaign.status, &filter) {
                    out.push_back(campaign);
                }
            }
        }
        out
    }

    pub fn get_campaign_stats(env: &Env, project_id: &String) -> Option<CampaignStats> {
        let campaign = Self::get_campaign(env, project_id)?;
        let raised_amount = Self::get_raised_amount(env, project_id);
        let donor_count = Self::count_donors(env, project_id);
        let donation_count = Self::get_donation_count(env, project_id);
        let progress_percentage = if campaign.goal_amount <= 0 {
            0
        } else {
            let numerator = raised_amount.saturating_mul(100);
            let ratio = numerator / campaign.goal_amount;
            if ratio < 0 {
                0
            } else if ratio > 100 {
                100
            } else {
                ratio as u32
            }
        };

        let remaining_amount = if raised_amount >= campaign.goal_amount {
            0
        } else {
            campaign.goal_amount - raised_amount
        };

        Some(CampaignStats {
            project_id: project_id.clone(),
            status: campaign.status,
            goal_amount: campaign.goal_amount,
            raised_amount,
            progress_percentage,
            donor_count,
            donation_count,
            remaining_amount,
        })
    }

    pub fn is_refund_enabled(env: &Env, project_id: &String) -> bool {
        if let Some(campaign) = Self::get_campaign(env, project_id) {
            campaign.status == CampaignStatus::Cancelled
        } else {
            false
        }
    }

    pub fn validate_donation_allowed(
        env: &Env,
        project_id: &String,
        now: u64,
    ) -> Result<(), &'static str> {
        let mut campaign = Self::get_campaign(env, project_id).ok_or("Campaign not found")?;
        let next_status = Self::resolve_status(&campaign, now);
        if campaign.status != next_status {
            campaign.status = next_status;
            campaign.updated_at = now;
            Self::save_campaign(env, &campaign);
        }

        match campaign.status {
            CampaignStatus::Active => Ok(()),
            CampaignStatus::Pending => Err("Campaign has not started"),
            CampaignStatus::Completed => Err("Campaign already completed"),
            CampaignStatus::Cancelled => Err("Campaign was cancelled"),
            CampaignStatus::Expired => Err("Campaign expired"),
        }
    }

    pub fn record_donation(env: &Env, project_id: &String, donor: &Address, amount: i128) {
        let mut raised_amount = Self::get_raised_amount(env, project_id);
        raised_amount += amount;
        env.storage()
            .instance()
            .set(&CampaignStorageKey::RaisedAmount(project_id.clone()), &raised_amount);

        let mut donation_count = Self::get_donation_count(env, project_id);
        donation_count += 1;
        env.storage()
            .instance()
            .set(&CampaignStorageKey::DonationCount(project_id.clone()), &donation_count);

        let donor_key = CampaignStorageKey::Donor(project_id.clone(), donor.clone());
        if !env.storage().instance().has(&donor_key) {
            env.storage().instance().set(&donor_key, &true);
            let mut donor_count = Self::count_donors(env, project_id);
            donor_count += 1;
            env.storage()
                .instance()
                .set(&CampaignStorageKey::DonorCount(project_id.clone()), &donor_count);
        }

        if let Some(mut campaign) = Self::get_campaign(env, project_id) {
            if campaign.status != CampaignStatus::Cancelled
                && campaign.status != CampaignStatus::Expired
                && raised_amount >= campaign.goal_amount
            {
                campaign.status = CampaignStatus::Completed;
                campaign.updated_at = env.ledger().timestamp();
                Self::save_campaign(env, &campaign);
            }
        }
    }

    fn validate_campaign_creation(
        env: &Env,
        project_id: &String,
        title: &String,
        description: &String,
        goal_amount: i128,
        goal_asset: &String,
        start_timestamp: u64,
        end_timestamp: u64,
    ) -> Result<(), &'static str> {
        if project_id.is_empty() {
            return Err("Project ID cannot be empty");
        }
        if title.is_empty() {
            return Err("Title cannot be empty");
        }
        if description.is_empty() {
            return Err("Description cannot be empty");
        }
        if goal_amount <= 0 {
            return Err("Goal amount must be positive");
        }
        if end_timestamp <= start_timestamp {
            return Err("End timestamp must be greater than start timestamp");
        }
        if !AssetResolver::is_supported(env, &goal_asset.to_string()) {
            return Err("Goal asset is not supported");
        }
        Ok(())
    }

    fn set_status(
        env: &Env,
        project_id: &String,
        status: CampaignStatus,
    ) -> Result<Campaign, &'static str> {
        let mut campaign = Self::get_campaign(env, project_id).ok_or("Campaign not found")?;
        if campaign.status == CampaignStatus::Cancelled && status != CampaignStatus::Cancelled {
            return Err("Cancelled campaign cannot transition to another state");
        }
        if campaign.status == CampaignStatus::Expired {
            return Err("Expired campaign cannot be changed");
        }
        campaign.status = status;
        campaign.updated_at = env.ledger().timestamp();
        Self::save_campaign(env, &campaign);
        Ok(campaign)
    }

    fn matches_filter(status: &CampaignStatus, filter: &CampaignListFilter) -> bool {
        match filter {
            CampaignListFilter::All => true,
            CampaignListFilter::Pending => *status == CampaignStatus::Pending,
            CampaignListFilter::Active => *status == CampaignStatus::Active,
            CampaignListFilter::Completed => *status == CampaignStatus::Completed,
            CampaignListFilter::Cancelled => *status == CampaignStatus::Cancelled,
            CampaignListFilter::Expired => *status == CampaignStatus::Expired,
        }
    }

    fn resolve_status(campaign: &Campaign, now: u64) -> CampaignStatus {
        match campaign.status {
            CampaignStatus::Completed | CampaignStatus::Cancelled | CampaignStatus::Expired => {
                campaign.status
            }
            CampaignStatus::Pending => {
                if now > campaign.end_timestamp {
                    CampaignStatus::Expired
                } else if now >= campaign.start_timestamp {
                    CampaignStatus::Active
                } else {
                    CampaignStatus::Pending
                }
            }
            CampaignStatus::Active => {
                if now > campaign.end_timestamp {
                    CampaignStatus::Expired
                } else {
                    CampaignStatus::Active
                }
            }
        }
    }

    fn save_campaign(env: &Env, campaign: &Campaign) {
        env.storage()
            .instance()
            .set(&CampaignStorageKey::Campaign(campaign.project_id.clone()), campaign);
    }

    fn append_campaign_id(env: &Env, project_id: &String) {
        let mut ids = Self::get_campaign_ids(env);
        ids.push_back(project_id.clone());
        env.storage().instance().set(&CampaignStorageKey::CampaignIds, &ids);
    }

    fn get_campaign_ids(env: &Env) -> Vec<String> {
        env.storage()
            .instance()
            .get(&CampaignStorageKey::CampaignIds)
            .unwrap_or_else(|| Vec::new(env))
    }

    fn get_raised_amount(env: &Env, project_id: &String) -> i128 {
        env.storage()
            .instance()
            .get(&CampaignStorageKey::RaisedAmount(project_id.clone()))
            .unwrap_or(0)
    }

    fn get_donation_count(env: &Env, project_id: &String) -> u32 {
        env.storage()
            .instance()
            .get(&CampaignStorageKey::DonationCount(project_id.clone()))
            .unwrap_or(0)
    }

    fn count_donors(env: &Env, project_id: &String) -> u32 {
        env.storage()
            .instance()
            .get(&CampaignStorageKey::DonorCount(project_id.clone()))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::vec;
    use soroban_sdk::testutils::Address as _;

    fn create_base_campaign(env: &Env, admin: &Address) -> String {
        let project_id = String::from_str(env, "proj-campaign-1");
        let tags = vec![env, String::from_str(env, "health")];
        let created = CampaignManager::create_campaign(
            env,
            admin,
            project_id.clone(),
            String::from_str(env, "Medical Relief"),
            String::from_str(env, "Support emergency aid"),
            Address::generate(env),
            10_000,
            String::from_str(env, "XLM"),
            100,
            300,
            String::from_str(env, "emergency"),
            tags,
        );
        assert!(created.is_ok());
        project_id
    }

    #[test]
    fn test_create_campaign_valid() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 50);
        let admin = Address::generate(&env);
        Rbac::set_admin(&env, &admin);

        let project_id = create_base_campaign(&env, &admin);
        let campaign = CampaignManager::get_campaign(&env, &project_id).unwrap();
        assert_eq!(campaign.status, CampaignStatus::Pending);
    }

    #[test]
    fn test_update_and_complete_campaign() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 120);
        let admin = Address::generate(&env);
        Rbac::set_admin(&env, &admin);
        let project_id = create_base_campaign(&env, &admin);

        let updated = CampaignManager::update_campaign(
            &env,
            &admin,
            project_id.clone(),
            String::from_str(&env, "Updated title"),
            String::from_str(&env, "Updated description"),
            Address::generate(&env),
            String::from_str(&env, "women"),
            vec![&env, String::from_str(&env, "urgent")],
        );
        assert!(updated.is_ok());

        let completed = CampaignManager::complete_campaign(&env, &admin, project_id.clone());
        assert!(completed.is_ok());
        assert_eq!(completed.unwrap().status, CampaignStatus::Completed);
    }

    #[test]
    fn test_cancel_campaign() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 120);
        let admin = Address::generate(&env);
        Rbac::set_admin(&env, &admin);
        let project_id = create_base_campaign(&env, &admin);

        let cancelled = CampaignManager::cancel_campaign(&env, &admin, project_id.clone()).unwrap();
        assert_eq!(cancelled.status, CampaignStatus::Cancelled);
    }

    #[test]
    fn test_expiration_logic() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 50);
        let admin = Address::generate(&env);
        Rbac::set_admin(&env, &admin);
        let project_id = create_base_campaign(&env, &admin);

        env.ledger().with_mut(|l| l.timestamp = 301);
        let campaign = CampaignManager::get_campaign(&env, &project_id).unwrap();
        assert_eq!(campaign.status, CampaignStatus::Expired);
    }

    #[test]
    fn test_list_campaigns_by_filter() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 50);
        let admin = Address::generate(&env);
        Rbac::set_admin(&env, &admin);
        let pending_project = create_base_campaign(&env, &admin);

        env.ledger().with_mut(|l| l.timestamp = 120);
        let active_project = String::from_str(&env, "proj-campaign-2");
        let created = CampaignManager::create_campaign(
            &env,
            &admin,
            active_project.clone(),
            String::from_str(&env, "Food Support"),
            String::from_str(&env, "Community meals"),
            Address::generate(&env),
            20_000,
            String::from_str(&env, "XLM"),
            100,
            500,
            String::from_str(&env, "community"),
            vec![&env, String::from_str(&env, "food")],
        );
        assert!(created.is_ok());

        let pending = CampaignManager::list_campaigns(&env, CampaignListFilter::Pending);
        let active = CampaignManager::list_campaigns(&env, CampaignListFilter::Active);
        let all = CampaignManager::list_campaigns(&env, CampaignListFilter::All);

        let mut pending_found = false;
        for campaign in pending.iter() {
            if campaign.project_id == pending_project {
                pending_found = true;
            }
        }

        let mut active_found = false;
        for campaign in active.iter() {
            if campaign.project_id == active_project {
                active_found = true;
            }
        }

        assert!(pending_found);
        assert!(active_found);
        assert_eq!(all.len(), 2);
    }

    #[test]
    #[should_panic]
    fn test_non_admin_cannot_create_campaign() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let non_admin = Address::generate(&env);
        Rbac::set_admin(&env, &admin);

        let _ = CampaignManager::create_campaign(
            &env,
            &non_admin,
            String::from_str(&env, "proj-unauthorized"),
            String::from_str(&env, "Unauthorized"),
            String::from_str(&env, "Should fail"),
            Address::generate(&env),
            1_000,
            String::from_str(&env, "XLM"),
            100,
            200,
            String::from_str(&env, "test"),
            vec![&env, String::from_str(&env, "x")],
        );
    }

    #[test]
    fn test_stats_tracking_and_auto_complete() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 120);
        let admin = Address::generate(&env);
        Rbac::set_admin(&env, &admin);
        let project_id = create_base_campaign(&env, &admin);

        let donor_a = Address::generate(&env);
        let donor_b = Address::generate(&env);

        CampaignManager::record_donation(&env, &project_id, &donor_a, 4_000);
        CampaignManager::record_donation(&env, &project_id, &donor_b, 6_000);

        let stats = CampaignManager::get_campaign_stats(&env, &project_id).unwrap();
        assert_eq!(stats.raised_amount, 10_000);
        assert_eq!(stats.progress_percentage, 100);
        assert_eq!(stats.donor_count, 2);

        let campaign = CampaignManager::get_campaign(&env, &project_id).unwrap();
        assert_eq!(campaign.status, CampaignStatus::Completed);
    }

    #[test]
    fn test_refund_enabled_after_cancel() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 120);
        let admin = Address::generate(&env);
        Rbac::set_admin(&env, &admin);
        let project_id = create_base_campaign(&env, &admin);

        let _ = CampaignManager::cancel_campaign(&env, &admin, project_id.clone()).unwrap();
        assert!(CampaignManager::is_refund_enabled(&env, &project_id));
    }
}