use crate::types::{
    CampaignStatus,
    CampaignStatusResponse,
};

pub fn get_campaign_status(
    env: Env,
) -> CampaignStatusResponse {
    let campaign = CampaignStorage::get(&env);

    let now = env.ledger().timestamp() as i64;
    let deadline = campaign.deadline as i64;

    let seconds_remaining = deadline - now;
    let days_remaining = seconds_remaining / 86_400;

    let status = if campaign.cancelled {
        CampaignStatus::Cancelled
    } else if campaign.raised_amount >= campaign.goal_amount {
        CampaignStatus::Successful
    } else if now > deadline {
        CampaignStatus::Failed
    } else {
        CampaignStatus::Active
    };

    CampaignStatusResponse {
        status,
        days_remaining,
    }
}

pub fn extend_deadline(
    env: Env,
    new_end_time: u64,
) {