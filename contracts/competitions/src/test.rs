extern crate std;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, vec, Address, Bytes, Env, String, Vec,
};

use crate::errors::CompetitionError;
use crate::types::{CompetitionRules, CompetitionStatus};
use crate::{Competitions, CompetitionsClient};

const HISTORY_LIMIT: u32 = 3;
const SUBMISSION_LEDGERS: u32 = 100;
const VOTING_LEDGERS: u32 = 100;
const PRIZE_POOL: i128 = 10_000;
const MIN_REPUTATION: u32 = 10;

struct Fixture<'a> {
    env: Env,
    client: CompetitionsClient<'a>,
    token: Address,
    organizer: Address,
    alice: Address,
    bob: Address,
    carol: Address,
    voter_one: Address,
    voter_two: Address,
    id: Bytes,
}

fn setup<'a>() -> Fixture<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let organizer = Address::generate(&env);
    token::StellarAssetClient::new(&env, &token).mint(&organizer, &1_000_000);

    let contract_id = env.register_contract(None, Competitions);
    let client = CompetitionsClient::new(&env, &contract_id);
    client.initialize(&admin, &HISTORY_LIMIT);

    Fixture {
        id: Bytes::from_slice(&env, b"comp-001"),
        alice: Address::generate(&env),
        bob: Address::generate(&env),
        carol: Address::generate(&env),
        voter_one: Address::generate(&env),
        voter_two: Address::generate(&env),
        env,
        client,
        token,
        organizer,
    }
}

impl Fixture<'_> {
    fn rules(&self, splits: Vec<u32>) -> CompetitionRules {
        CompetitionRules {
            submission_ledgers: SUBMISSION_LEDGERS,
            voting_ledgers: VOTING_LEDGERS,
            max_submissions: 3,
            min_reputation: MIN_REPUTATION,
            prize_split_bps: splits,
        }
    }

    fn create(&self) {
        self.client.create_competition(
            &self.id,
            &self.organizer,
            &self.token,
            &String::from_str(&self.env, "Poster jam"),
            &PRIZE_POOL,
            &self.rules(vec![&self.env, 6_000, 4_000]),
        );
    }

    fn submit(&self, entrant: &Address) {
        self.client.submit(
            &self.id,
            entrant,
            &String::from_str(&self.env, "ipfs://entry"),
        );
    }

    fn open_voting(&self) {
        self.env
            .ledger()
            .with_mut(|l| l.sequence_number += SUBMISSION_LEDGERS + 1);
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
fn create_competition_escrows_the_prize_pool() {
    let f = setup();
    let before = f.balance(&f.organizer);
    f.create();

    let competition = f.client.get_competition(&f.id);
    assert_eq!(competition.organizer, f.organizer);
    assert_eq!(competition.prize_pool, PRIZE_POOL);
    assert_eq!(competition.status, CompetitionStatus::Open);
    assert_eq!(f.balance(&f.organizer), before - PRIZE_POOL);
}

#[test]
fn invalid_rules_are_rejected() {
    let f = setup();
    let title = String::from_str(&f.env, "Bad");
    // Splits that do not total 10000.
    let err = f
        .client
        .try_create_competition(
            &f.id,
            &f.organizer,
            &f.token,
            &title,
            &PRIZE_POOL,
            &f.rules(vec![&f.env, 6_000, 3_000]),
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, CompetitionError::InvalidRules);

    // No prize positions at all.
    let err = f
        .client
        .try_create_competition(
            &f.id,
            &f.organizer,
            &f.token,
            &title,
            &PRIZE_POOL,
            &f.rules(Vec::new(&f.env)),
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, CompetitionError::InvalidRules);
}

#[test]
fn a_non_positive_prize_pool_is_rejected() {
    let f = setup();
    let err = f
        .client
        .try_create_competition(
            &f.id,
            &f.organizer,
            &f.token,
            &String::from_str(&f.env, "Free"),
            &0,
            &f.rules(vec![&f.env, 10_000]),
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, CompetitionError::InvalidPrizePool);
}

#[test]
fn submissions_are_tracked_and_deduplicated() {
    let f = setup();
    f.create();
    f.submit(&f.alice);

    let submission = f.client.get_submission(&f.id, &f.alice);
    assert_eq!(submission.entrant, f.alice);
    assert_eq!(submission.votes, 0);
    assert_eq!(f.client.get_competition(&f.id).submission_count, 1);
    assert_eq!(f.client.get_entrants(&f.id).len(), 1);

    let err = f
        .client
        .try_submit(&f.id, &f.alice, &String::from_str(&f.env, "ipfs://again"))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, CompetitionError::AlreadySubmitted);
}

#[test]
fn the_submission_cap_is_enforced() {
    let f = setup();
    f.create();
    f.submit(&f.alice);
    f.submit(&f.bob);
    f.submit(&f.carol);
    let err = f
        .client
        .try_submit(
            &f.id,
            &Address::generate(&f.env),
            &String::from_str(&f.env, "ipfs://late"),
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, CompetitionError::TooManySubmissions);
}

#[test]
fn submissions_close_when_the_window_ends() {
    let f = setup();
    f.create();
    f.open_voting();
    let err = f
        .client
        .try_submit(&f.id, &f.alice, &String::from_str(&f.env, "ipfs://late"))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, CompetitionError::SubmissionsClosed);
}

#[test]
fn voting_is_weighted_by_reputation() {
    let f = setup();
    f.create();
    f.submit(&f.alice);
    f.submit(&f.bob);
    f.client.set_reputation(&f.voter_one, &30);
    f.client.set_reputation(&f.voter_two, &70);
    f.open_voting();

    assert_eq!(f.client.vote(&f.id, &f.voter_one, &f.alice), 30);
    assert_eq!(f.client.vote(&f.id, &f.voter_two, &f.bob), 70);

    assert_eq!(f.client.get_submission(&f.id, &f.alice).votes, 30);
    assert_eq!(f.client.get_submission(&f.id, &f.bob).votes, 70);
    assert_eq!(f.client.get_competition(&f.id).total_votes, 100);
}

#[test]
fn low_reputation_voters_are_turned_away() {
    let f = setup();
    f.create();
    f.submit(&f.alice);
    f.client.set_reputation(&f.voter_one, &(MIN_REPUTATION - 1));
    f.open_voting();

    let err = f
        .client
        .try_vote(&f.id, &f.voter_one, &f.alice)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, CompetitionError::ReputationTooLow);

    // An account with no reputation at all is also refused.
    let err = f
        .client
        .try_vote(&f.id, &f.voter_two, &f.alice)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, CompetitionError::ReputationTooLow);
}

#[test]
fn double_voting_and_self_voting_are_rejected() {
    let f = setup();
    f.create();
    f.submit(&f.alice);
    f.submit(&f.bob);
    f.client.set_reputation(&f.voter_one, &30);
    f.client.set_reputation(&f.alice, &30);
    f.open_voting();

    f.client.vote(&f.id, &f.voter_one, &f.alice);
    let err = f
        .client
        .try_vote(&f.id, &f.voter_one, &f.bob)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, CompetitionError::AlreadyVoted);

    let err = f
        .client
        .try_vote(&f.id, &f.alice, &f.alice)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, CompetitionError::SelfVoteNotAllowed);
}

#[test]
fn the_voting_window_is_enforced_at_both_ends() {
    let f = setup();
    f.create();
    f.submit(&f.alice);
    f.client.set_reputation(&f.voter_one, &30);

    let err = f
        .client
        .try_vote(&f.id, &f.voter_one, &f.alice)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, CompetitionError::VotingNotOpen);

    f.open_voting();
    f.close_voting();
    let err = f
        .client
        .try_vote(&f.id, &f.voter_one, &f.alice)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, CompetitionError::VotingClosed);
}

#[test]
fn finalizing_ranks_entries_and_assigns_prizes() {
    let f = setup();
    f.create();
    f.submit(&f.alice);
    f.submit(&f.bob);
    f.client.set_reputation(&f.voter_one, &30);
    f.client.set_reputation(&f.voter_two, &70);
    f.open_voting();
    f.client.vote(&f.id, &f.voter_one, &f.alice);
    f.client.vote(&f.id, &f.voter_two, &f.bob);
    f.close_voting();

    let winners = f.client.finalize(&f.id);
    assert_eq!(winners.len(), 2);
    // Bob's 70 weighted votes beat Alice's 30.
    assert_eq!(winners.get(0).unwrap().entrant, f.bob);
    assert_eq!(winners.get(0).unwrap().rank, 1);
    assert_eq!(winners.get(0).unwrap().prize, 6_000);
    assert_eq!(winners.get(1).unwrap().entrant, f.alice);
    assert_eq!(winners.get(1).unwrap().prize, 4_000);
    assert_eq!(
        f.client.get_competition(&f.id).status,
        CompetitionStatus::Finalized
    );
}

#[test]
fn finalizing_before_voting_closes_is_rejected() {
    let f = setup();
    f.create();
    f.submit(&f.alice);
    f.open_voting();
    let err = f.client.try_finalize(&f.id).err().unwrap().unwrap();
    assert_eq!(err, CompetitionError::VotingClosed);
}

#[test]
fn a_competition_can_only_be_finalized_once() {
    let f = setup();
    f.create();
    f.submit(&f.alice);
    f.open_voting();
    f.close_voting();
    f.client.finalize(&f.id);
    let err = f.client.try_finalize(&f.id).err().unwrap().unwrap();
    assert_eq!(err, CompetitionError::AlreadyFinalized);
}

#[test]
fn prizes_are_paid_to_the_ranked_winners() {
    let f = setup();
    f.create();
    f.submit(&f.alice);
    f.submit(&f.bob);
    f.client.set_reputation(&f.voter_one, &70);
    f.open_voting();
    f.client.vote(&f.id, &f.voter_one, &f.alice);
    f.close_voting();
    f.client.finalize(&f.id);

    f.client.distribute_prizes(&f.id);
    assert_eq!(f.balance(&f.alice), 6_000);
    assert_eq!(f.balance(&f.bob), 4_000);
    assert_eq!(f.balance(&f.client.address), 0);
    assert_eq!(
        f.client.get_competition(&f.id).status,
        CompetitionStatus::Settled
    );
}

#[test]
fn unfilled_positions_return_to_the_organizer() {
    let f = setup();
    f.create();
    // Only one entry for a two-position prize split.
    f.submit(&f.alice);
    let organizer_before = f.balance(&f.organizer);
    f.open_voting();
    f.close_voting();
    f.client.finalize(&f.id);
    f.client.distribute_prizes(&f.id);

    assert_eq!(f.balance(&f.alice), 6_000);
    assert_eq!(f.balance(&f.organizer), organizer_before + 4_000);
    assert_eq!(f.balance(&f.client.address), 0);
}

#[test]
fn a_competition_with_no_entries_refunds_the_whole_pool() {
    let f = setup();
    f.create();
    let organizer_before = f.balance(&f.organizer);
    f.open_voting();
    f.close_voting();

    let winners = f.client.finalize(&f.id);
    assert!(winners.is_empty());
    f.client.distribute_prizes(&f.id);
    assert_eq!(f.balance(&f.organizer), organizer_before + PRIZE_POOL);
}

#[test]
fn rounding_dust_joins_the_top_prize() {
    let f = setup();
    f.client.create_competition(
        &f.id,
        &f.organizer,
        &f.token,
        &String::from_str(&f.env, "Thirds"),
        &10,
        &f.rules(vec![&f.env, 3_333, 3_333, 3_334]),
    );
    f.submit(&f.alice);
    f.submit(&f.bob);
    f.submit(&f.carol);
    f.open_voting();
    f.close_voting();
    f.client.finalize(&f.id);
    f.client.distribute_prizes(&f.id);

    // 10 split three ways floors to 3+3+3; the spare unit goes to first place.
    assert_eq!(f.balance(&f.client.address), 0);
    let winners = f.client.get_winners(&f.id);
    let total: i128 = winners.iter().map(|w| w.prize).sum();
    assert_eq!(total, 10);
    assert_eq!(winners.get(0).unwrap().prize, 4);
}

#[test]
fn prizes_cannot_be_distributed_twice_or_before_finalizing() {
    let f = setup();
    f.create();
    f.submit(&f.alice);
    let err = f
        .client
        .try_distribute_prizes(&f.id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, CompetitionError::NotFinalized);

    f.open_voting();
    f.close_voting();
    f.client.finalize(&f.id);
    f.client.distribute_prizes(&f.id);
    let err = f
        .client
        .try_distribute_prizes(&f.id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, CompetitionError::AlreadySettled);
}

#[test]
fn history_records_each_finalized_competition() {
    let f = setup();
    f.create();
    f.submit(&f.alice);
    f.open_voting();
    f.close_voting();
    f.client.finalize(&f.id);

    let history = f.client.get_history();
    assert_eq!(history.len(), 1);
    let entry = history.get(0).unwrap();
    assert_eq!(entry.competition_id, f.id);
    assert_eq!(entry.prize_pool, PRIZE_POOL);
    assert_eq!(entry.submission_count, 1);
    assert_eq!(entry.top_entrant, Some(f.alice.clone()));
}

#[test]
fn history_is_capped_at_the_configured_limit() {
    let f = setup();
    for i in 0..HISTORY_LIMIT + 2 {
        let id = Bytes::from_slice(&f.env, &[b'c', b'0' + i as u8]);
        f.client.create_competition(
            &id,
            &f.organizer,
            &f.token,
            &String::from_str(&f.env, "Jam"),
            &PRIZE_POOL,
            &f.rules(vec![&f.env, 10_000]),
        );
        f.open_voting();
        f.close_voting();
        f.client.finalize(&id);
    }
    assert_eq!(f.client.get_history().len(), HISTORY_LIMIT);
}

#[test]
fn duplicate_competition_ids_are_rejected() {
    let f = setup();
    f.create();
    let err = f
        .client
        .try_create_competition(
            &f.id,
            &f.organizer,
            &f.token,
            &String::from_str(&f.env, "Dup"),
            &PRIZE_POOL,
            &f.rules(vec![&f.env, 10_000]),
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, CompetitionError::CompetitionExists);
}

#[test]
fn unknown_competitions_and_submissions_are_reported() {
    let f = setup();
    let missing = Bytes::from_slice(&f.env, b"nope");
    assert_eq!(
        f.client
            .try_get_competition(&missing)
            .err()
            .unwrap()
            .unwrap(),
        CompetitionError::CompetitionNotFound
    );
    f.create();
    assert_eq!(
        f.client
            .try_get_submission(&f.id, &f.alice)
            .err()
            .unwrap()
            .unwrap(),
        CompetitionError::SubmissionNotFound
    );
}
