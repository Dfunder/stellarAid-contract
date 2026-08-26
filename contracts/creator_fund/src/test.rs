extern crate std;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Bytes, Env, String,
};

use crate::errors::FundError;
use crate::types::{DistributionRule, FundType, ProposalStatus};
use crate::{CreatorFund, CreatorFundClient};

const HISTORY_LIMIT: u32 = 5;
const VOTING_LEDGERS: u32 = 100;

struct Fixture<'a> {
    env: Env,
    client: CreatorFundClient<'a>,
    token: Address,
    steward: Address,
    alice: Address,
    bob: Address,
    grantee: Address,
    fund_id: Bytes,
    proposal_id: Bytes,
}

fn rule(max_allocation_bps: u32, min_reserve: i128, quorum_bps: u32) -> DistributionRule {
    DistributionRule {
        max_allocation_bps,
        min_reserve,
        quorum_bps,
        voting_ledgers: VOTING_LEDGERS,
    }
}

fn setup<'a>() -> Fixture<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let steward = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let grantee = Address::generate(&env);

    let minter = token::StellarAssetClient::new(&env, &token);
    minter.mint(&alice, &10_000);
    minter.mint(&bob, &10_000);

    let contract_id = env.register_contract(None, CreatorFund);
    let client = CreatorFundClient::new(&env, &contract_id);
    client.initialize(&admin, &HISTORY_LIMIT);

    Fixture {
        fund_id: Bytes::from_slice(&env, b"grants-2026"),
        proposal_id: Bytes::from_slice(&env, b"prop-001"),
        env,
        client,
        token,
        steward,
        alice,
        bob,
        grantee,
    }
}

impl Fixture<'_> {
    fn create(&self, fund_type: FundType, rule: DistributionRule) {
        self.client
            .create_fund(&self.fund_id, &fund_type, &self.steward, &self.token, &rule);
    }

    fn fund_with_capital(&self) {
        self.create(FundType::GrantPool, rule(5000, 100, 5000));
        self.client.contribute(&self.fund_id, &self.alice, &1_000);
        self.client.contribute(&self.fund_id, &self.bob, &1_000);
    }

    fn propose(&self, amount: i128) {
        self.client.propose_allocation(
            &self.proposal_id,
            &self.fund_id,
            &self.alice,
            &self.grantee,
            &amount,
            &String::from_str(&self.env, "emerging artist grant"),
        );
    }

    fn close_voting(&self) {
        self.env
            .ledger()
            .with_mut(|l| l.sequence_number += VOTING_LEDGERS + 1);
    }

    fn balance(&self, account: &Address) -> i128 {
        token::Client::new(&self.env, &self.token).balance(account)
    }
}

#[test]
fn funds_of_every_type_can_be_opened() {
    let f = setup();
    for (i, fund_type) in [
        FundType::GrantPool,
        FundType::EmergencyRelief,
        FundType::PlatformInitiative,
        FundType::MatchingPool,
    ]
    .iter()
    .enumerate()
    {
        let id = Bytes::from_slice(&f.env, &[b'f', b'0' + i as u8]);
        f.client
            .create_fund(&id, fund_type, &f.steward, &f.token, &rule(5000, 0, 5000));
        assert_eq!(f.client.get_fund(&id).fund_type, *fund_type);
    }
}

#[test]
fn duplicate_fund_id_is_rejected() {
    let f = setup();
    f.create(FundType::GrantPool, rule(5000, 0, 5000));
    let err = f
        .client
        .try_create_fund(
            &f.fund_id,
            &FundType::MatchingPool,
            &f.steward,
            &f.token,
            &rule(5000, 0, 5000),
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, FundError::FundExists);
}

#[test]
fn invalid_rules_are_rejected() {
    let f = setup();
    for bad in [
        rule(0, 0, 5000),
        rule(10_001, 0, 5000),
        rule(5000, 0, 10_001),
    ] {
        let err = f
            .client
            .try_create_fund(&f.fund_id, &FundType::GrantPool, &f.steward, &f.token, &bad)
            .err()
            .unwrap()
            .unwrap();
        assert_eq!(err, FundError::InvalidRule);
    }
}

#[test]
fn contributions_move_tokens_and_track_growth() {
    let f = setup();
    f.fund_with_capital();

    let fund = f.client.get_fund(&f.fund_id);
    assert_eq!(fund.balance, 2_000);
    assert_eq!(fund.total_contributed, 2_000);
    assert_eq!(fund.contributor_count, 2);
    assert_eq!(f.client.get_contribution(&f.fund_id, &f.alice), 1_000);
    assert_eq!(f.balance(&f.alice), 9_000);

    // One snapshot at creation plus one per contribution.
    let growth = f.client.get_growth(&f.fund_id);
    assert_eq!(growth.len(), 3);
    assert_eq!(growth.get(0).unwrap().balance, 0);
    assert_eq!(growth.get(2).unwrap().balance, 2_000);
}

#[test]
fn repeat_contributions_do_not_double_count_contributors() {
    let f = setup();
    f.fund_with_capital();
    f.client.contribute(&f.fund_id, &f.alice, &500);
    let fund = f.client.get_fund(&f.fund_id);
    assert_eq!(fund.contributor_count, 2);
    assert_eq!(f.client.get_contribution(&f.fund_id, &f.alice), 1_500);
}

#[test]
fn non_contributors_cannot_propose_or_vote() {
    let f = setup();
    f.fund_with_capital();
    let outsider = Address::generate(&f.env);
    let err = f
        .client
        .try_propose_allocation(
            &f.proposal_id,
            &f.fund_id,
            &outsider,
            &f.grantee,
            &500,
            &String::from_str(&f.env, "grant"),
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, FundError::NotContributor);

    f.propose(500);
    let err = f
        .client
        .try_vote(&f.proposal_id, &outsider, &true)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, FundError::NotContributor);
}

#[test]
fn approved_proposal_pays_out_and_records_the_allocation() {
    let f = setup();
    f.fund_with_capital();
    f.propose(500);

    assert_eq!(f.client.vote(&f.proposal_id, &f.alice, &true), 1_000);
    f.close_voting();
    assert_eq!(
        f.client.finalize_proposal(&f.proposal_id),
        ProposalStatus::Approved
    );

    f.client.execute_allocation(&f.proposal_id);

    let fund = f.client.get_fund(&f.fund_id);
    assert_eq!(fund.balance, 1_500);
    assert_eq!(fund.total_allocated, 500);
    assert_eq!(f.balance(&f.grantee), 500);

    let allocations = f.client.get_allocations(&f.fund_id);
    assert_eq!(allocations.len(), 1);
    assert_eq!(allocations.get(0).unwrap().recipient, f.grantee);
    assert_eq!(
        f.client.get_proposal(&f.proposal_id).status,
        ProposalStatus::Executed
    );
}

#[test]
fn votes_are_weighted_by_contribution() {
    let f = setup();
    f.create(FundType::GrantPool, rule(5000, 0, 5000));
    f.client.contribute(&f.fund_id, &f.alice, &300);
    f.client.contribute(&f.fund_id, &f.bob, &700);
    f.propose(100);

    f.client.vote(&f.proposal_id, &f.alice, &true);
    f.client.vote(&f.proposal_id, &f.bob, &false);
    f.close_voting();

    let proposal = f.client.get_proposal(&f.proposal_id);
    assert_eq!(proposal.votes_for, 300);
    assert_eq!(proposal.votes_against, 700);
    assert_eq!(
        f.client.finalize_proposal(&f.proposal_id),
        ProposalStatus::Rejected
    );
}

#[test]
fn quorum_shortfall_rejects_the_proposal() {
    let f = setup();
    f.create(FundType::GrantPool, rule(5000, 0, 9000));
    f.client.contribute(&f.fund_id, &f.alice, &1_000);
    f.client.contribute(&f.fund_id, &f.bob, &1_000);
    f.propose(100);

    // 1000 of 2000 contributed capital turns out; the rule demands 90%.
    f.client.vote(&f.proposal_id, &f.alice, &true);
    f.close_voting();
    assert_eq!(
        f.client.finalize_proposal(&f.proposal_id),
        ProposalStatus::Rejected
    );
    let err = f
        .client
        .try_execute_allocation(&f.proposal_id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, FundError::ProposalNotApproved);
}

#[test]
fn double_voting_is_rejected() {
    let f = setup();
    f.fund_with_capital();
    f.propose(500);
    f.client.vote(&f.proposal_id, &f.alice, &true);
    let err = f
        .client
        .try_vote(&f.proposal_id, &f.alice, &false)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, FundError::AlreadyVoted);
}

#[test]
fn voting_window_is_enforced_at_both_ends() {
    let f = setup();
    f.fund_with_capital();
    f.propose(500);

    let err = f
        .client
        .try_finalize_proposal(&f.proposal_id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, FundError::VotingOpen);

    f.close_voting();
    let err = f
        .client
        .try_vote(&f.proposal_id, &f.alice, &true)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, FundError::VotingClosed);
}

#[test]
fn allocation_cap_is_enforced() {
    let f = setup();
    f.create(FundType::GrantPool, rule(2000, 0, 5000));
    f.client.contribute(&f.fund_id, &f.alice, &1_000);
    // Cap is 20% of the 1000 balance.
    let err = f
        .client
        .try_propose_allocation(
            &f.proposal_id,
            &f.fund_id,
            &f.alice,
            &f.grantee,
            &201,
            &String::from_str(&f.env, "too big"),
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, FundError::ExceedsAllocationLimit);
}

#[test]
fn reserve_floor_is_enforced() {
    let f = setup();
    f.create(FundType::GrantPool, rule(10_000, 900, 5000));
    f.client.contribute(&f.fund_id, &f.alice, &1_000);
    let err = f
        .client
        .try_propose_allocation(
            &f.proposal_id,
            &f.fund_id,
            &f.alice,
            &f.grantee,
            &200,
            &String::from_str(&f.env, "breaches reserve"),
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, FundError::ReserveBreached);
}

#[test]
fn rule_is_rechecked_at_execution_time() {
    let f = setup();
    f.create(FundType::GrantPool, rule(10_000, 0, 5000));
    f.client.contribute(&f.fund_id, &f.alice, &1_000);
    f.propose(1_000);
    f.client.vote(&f.proposal_id, &f.alice, &true);
    f.close_voting();
    f.client.finalize_proposal(&f.proposal_id);

    // Tightening the reserve after approval blocks the payout.
    f.client.set_rule(&f.fund_id, &rule(10_000, 500, 5000));
    let err = f
        .client
        .try_execute_allocation(&f.proposal_id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, FundError::ReserveBreached);
    assert_eq!(f.client.get_fund(&f.fund_id).balance, 1_000);
}

#[test]
fn duplicate_proposal_id_is_rejected() {
    let f = setup();
    f.fund_with_capital();
    f.propose(500);
    let err = f
        .client
        .try_propose_allocation(
            &f.proposal_id,
            &f.fund_id,
            &f.alice,
            &f.grantee,
            &100,
            &String::from_str(&f.env, "duplicate"),
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, FundError::ProposalExists);
}

#[test]
fn unknown_fund_and_proposal_are_reported() {
    let f = setup();
    assert_eq!(
        f.client
            .try_get_fund(&Bytes::from_slice(&f.env, b"nope"))
            .err()
            .unwrap()
            .unwrap(),
        FundError::FundNotFound
    );
    assert_eq!(
        f.client
            .try_get_proposal(&Bytes::from_slice(&f.env, b"nope"))
            .err()
            .unwrap()
            .unwrap(),
        FundError::ProposalNotFound
    );
}

#[test]
fn growth_history_is_capped_at_the_configured_limit() {
    let f = setup();
    f.create(FundType::GrantPool, rule(5000, 0, 5000));
    for _ in 0..HISTORY_LIMIT + 2 {
        f.client.contribute(&f.fund_id, &f.alice, &100);
    }
    let growth = f.client.get_growth(&f.fund_id);
    assert_eq!(growth.len(), HISTORY_LIMIT);
    assert_eq!(
        growth.get(HISTORY_LIMIT - 1).unwrap().balance,
        100 * (HISTORY_LIMIT as i128 + 2)
    );
}
