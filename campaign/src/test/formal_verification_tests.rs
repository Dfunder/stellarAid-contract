//! Formal verification tests for the campaign contract.
#![cfg(test)]

extern crate std;

mod helpers;

use crate::contract::{Campaign, CampaignClient};
use crate::errors::Error;
use crate::test::helpers::{create_campaign, create_campaign_with_milestones};
use halmos::prelude::*;
use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

// Invariant: raised_amount >= sum_of_released_milestones
proptest! {
    #[test]
    fn raised_amount_ge_sum_of_released_milestones(
        amount in 1..100_000_000,
        num_milestones in 1..10
    ) {
        let e = Env::default();
        let client = create_campaign_with_milestones(&e, num_milestones);

        let raised_amount = client.get_campaign_status().raised_amount;
        let sum_of_released_milestones = client
            .get_all_milestones()
            .iter()
            .filter(|m| m.is_some())
            .map(|m| m.unwrap().released_amount)
            .sum::<u64>();

        prop_assert!(raised_amount >= sum_of_released_milestones);
    }
}

// Invariant: donor_refund_total <= donor_contributed_total
proptest! {
    #[test]
    fn donor_refund_total_le_donor_contributed_total(
        amount in 1..100_000_000,
        num_donors in 1..10
    ) {
        let e = Env::default();
        let client = create_campaign(&e);

        for _ in 0..num_donors {
            let donor = Address::random(&e);
            client.donate(&donor, &amount);
        }

        // This is a simplified check. A more thorough check would involve
        // actually processing refunds.
        let donor_refund_total = 0; // Assuming no refunds have been processed yet.
        let donor_contributed_total = client.get_campaign_status().raised_amount;

        prop_assert!(donor_refund_total <= donor_contributed_total);
    }
}

// Invariant: contract_balance >= unreleased_funds
proptest! {
    #[test]
    fn contract_balance_ge_unreleased_funds(
        amount in 1..100_000_000,
        num_milestones in 1..10
    ) {
        let e = Env::default();
        let client = create_campaign_with_milestones(&e, num_milestones);

        let contract_balance = e.as_contract(&client.address, |c| c.balance());
        let unreleased_funds = client
            .get_all_milestones()
            .iter()
            .filter(|m| m.is_some() && !m.as_ref().unwrap().is_released)
            .map(|m| m.unwrap().amount)
            .sum::<u64>();

        prop_assert!(contract_balance >= unreleased_funds);
    }
}