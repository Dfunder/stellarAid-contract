#![cfg(test)]

use crate::contract::{Campaign, CampaignClient};
use crate::types::Milestone;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

pub fn create_campaign(e: &Env) -> CampaignClient {
    let client = CampaignClient::new(e, &e.register_contract(None, Campaign));
    client.initialize(&Address::random(e), &String::from_str(e, "Test Campaign"));
    client
}

pub fn create_campaign_with_milestones(e: &Env, num_milestones: u32) -> CampaignClient {
    let client = create_campaign(e);
    for i in 0..num_milestones {
        client.add_milestone(&String::from_str(e, &format!("Milestone {}", i)), &100);
    }
    client
}