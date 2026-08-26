#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, token, Address, Bytes, Env, String, Vec};

pub mod errors;
pub mod types;

#[cfg(test)]
mod test;

use errors::CompetitionError;
use types::{
    Competition, CompetitionRules, CompetitionStatus, CompetitionSummary, DataKey, Submission,
    Winner, MAX_PRIZE_POSITIONS, TOTAL_BPS,
};

#[contract]
pub struct Competitions;

fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

fn require_admin(env: &Env) -> Result<(), CompetitionError> {
    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(CompetitionError::NotInitialized)?;
    admin.require_auth();
    Ok(())
}

fn require_initialized(env: &Env) -> Result<(), CompetitionError> {
    if has_admin(env) {
        Ok(())
    } else {
        Err(CompetitionError::NotInitialized)
    }
}

fn load_competition(env: &Env, id: &Bytes) -> Result<Competition, CompetitionError> {
    env.storage()
        .persistent()
        .get(&DataKey::Competition(id.clone()))
        .ok_or(CompetitionError::CompetitionNotFound)
}

fn save_competition(env: &Env, competition: &Competition) {
    env.storage()
        .persistent()
        .set(&DataKey::Competition(competition.id.clone()), competition);
}

fn load_submission(
    env: &Env,
    id: &Bytes,
    entrant: &Address,
) -> Result<Submission, CompetitionError> {
    env.storage()
        .persistent()
        .get(&DataKey::Submission(id.clone(), entrant.clone()))
        .ok_or(CompetitionError::SubmissionNotFound)
}

fn entrants_of(env: &Env, id: &Bytes) -> Vec<Address> {
    env.storage()
        .persistent()
        .get(&DataKey::Entrants(id.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

fn reputation_of(env: &Env, account: &Address) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::Reputation(account.clone()))
        .unwrap_or(0)
}

fn validate_rules(rules: &CompetitionRules) -> Result<(), CompetitionError> {
    if rules.submission_ledgers == 0
        || rules.voting_ledgers == 0
        || rules.max_submissions == 0
        || rules.prize_split_bps.is_empty()
        || rules.prize_split_bps.len() > MAX_PRIZE_POSITIONS
    {
        return Err(CompetitionError::InvalidRules);
    }
    let mut total: u32 = 0;
    for share in rules.prize_split_bps.iter() {
        if share == 0 {
            return Err(CompetitionError::InvalidRules);
        }
        total = total
            .checked_add(share)
            .ok_or(CompetitionError::ArithmeticOverflow)?;
    }
    if total != TOTAL_BPS {
        return Err(CompetitionError::InvalidRules);
    }
    Ok(())
}

/// Rank the entries by weighted votes, highest first. Ties break towards the
/// earlier submission, which falls out of scanning the entrant list in order
/// and only replacing the leader on a strictly greater score.
fn rank_entries(env: &Env, id: &Bytes) -> Vec<Submission> {
    let entrants = entrants_of(env, id);
    let mut remaining: Vec<Submission> = Vec::new(env);
    for entrant in entrants.iter() {
        if let Ok(submission) = load_submission(env, id, &entrant) {
            remaining.push_back(submission);
        }
    }

    let mut ranked: Vec<Submission> = Vec::new(env);
    while !remaining.is_empty() {
        let mut best = 0u32;
        for i in 1..remaining.len() {
            if remaining.get(i).unwrap().votes > remaining.get(best).unwrap().votes {
                best = i;
            }
        }
        ranked.push_back(remaining.get(best).unwrap());
        remaining.remove(best);
    }
    ranked
}

#[contractimpl]
impl Competitions {
    pub fn initialize(
        env: Env,
        admin: Address,
        history_limit: u32,
    ) -> Result<(), CompetitionError> {
        if has_admin(&env) {
            return Err(CompetitionError::AlreadyInitialized);
        }
        if history_limit == 0 {
            return Err(CompetitionError::InvalidRules);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::HistoryLimit, &history_limit);
        env.events()
            .publish((symbol_short!("init"),), (admin, history_limit));
        Ok(())
    }

    /// Publish an account's reputation score, which is the weight its vote
    /// carries. Admin-only: reputation is computed off-chain and attested here.
    pub fn set_reputation(env: Env, account: Address, score: u32) -> Result<(), CompetitionError> {
        require_admin(&env)?;
        env.storage()
            .persistent()
            .set(&DataKey::Reputation(account.clone()), &score);
        env.events()
            .publish((symbol_short!("rep_set"),), (account, score));
        Ok(())
    }

    pub fn get_reputation(env: Env, account: Address) -> u32 {
        reputation_of(&env, &account)
    }

    /// Open a competition and escrow its prize pool. The rules are fixed here
    /// and cannot be changed once entrants have committed work.
    pub fn create_competition(
        env: Env,
        id: Bytes,
        organizer: Address,
        token_address: Address,
        title: String,
        prize_pool: i128,
        rules: CompetitionRules,
    ) -> Result<(), CompetitionError> {
        require_initialized(&env)?;
        organizer.require_auth();

        if env
            .storage()
            .persistent()
            .has(&DataKey::Competition(id.clone()))
        {
            return Err(CompetitionError::CompetitionExists);
        }
        if prize_pool <= 0 {
            return Err(CompetitionError::InvalidPrizePool);
        }
        validate_rules(&rules)?;

        let ledger = env.ledger().sequence();
        let submission_end_ledger = ledger + rules.submission_ledgers;
        let competition = Competition {
            id: id.clone(),
            organizer: organizer.clone(),
            token: token_address.clone(),
            title,
            prize_pool,
            submission_end_ledger,
            voting_end_ledger: submission_end_ledger + rules.voting_ledgers,
            rules,
            status: CompetitionStatus::Open,
            submission_count: 0,
            total_votes: 0,
            created_ledger: ledger,
        };
        save_competition(&env, &competition);

        token::Client::new(&env, &token_address).transfer(
            &organizer,
            &env.current_contract_address(),
            &prize_pool,
        );

        env.events()
            .publish((symbol_short!("comp_new"),), (id, organizer, prize_pool));
        Ok(())
    }

    pub fn submit(
        env: Env,
        competition_id: Bytes,
        entrant: Address,
        entry_uri: String,
    ) -> Result<(), CompetitionError> {
        require_initialized(&env)?;
        entrant.require_auth();

        let mut competition = load_competition(&env, &competition_id)?;
        if competition.status != CompetitionStatus::Open
            || env.ledger().sequence() > competition.submission_end_ledger
        {
            return Err(CompetitionError::SubmissionsClosed);
        }
        if env.storage().persistent().has(&DataKey::Submission(
            competition_id.clone(),
            entrant.clone(),
        )) {
            return Err(CompetitionError::AlreadySubmitted);
        }
        if competition.submission_count >= competition.rules.max_submissions {
            return Err(CompetitionError::TooManySubmissions);
        }

        let submission = Submission {
            competition_id: competition_id.clone(),
            entrant: entrant.clone(),
            entry_uri,
            votes: 0,
            voter_count: 0,
            submitted_ledger: env.ledger().sequence(),
        };
        env.storage().persistent().set(
            &DataKey::Submission(competition_id.clone(), entrant.clone()),
            &submission,
        );

        let mut entrants = entrants_of(&env, &competition_id);
        entrants.push_back(entrant.clone());
        env.storage()
            .persistent()
            .set(&DataKey::Entrants(competition_id.clone()), &entrants);

        competition.submission_count += 1;
        save_competition(&env, &competition);

        env.events()
            .publish((symbol_short!("submit"),), (competition_id, entrant));
        Ok(())
    }

    /// Cast a reputation-weighted vote. One ballot per voter per competition,
    /// and entrants may not vote for their own entry.
    pub fn vote(
        env: Env,
        competition_id: Bytes,
        voter: Address,
        entrant: Address,
    ) -> Result<u32, CompetitionError> {
        require_initialized(&env)?;
        voter.require_auth();

        let mut competition = load_competition(&env, &competition_id)?;
        if competition.status != CompetitionStatus::Open {
            return Err(CompetitionError::VotingClosed);
        }
        let ledger = env.ledger().sequence();
        if ledger <= competition.submission_end_ledger {
            return Err(CompetitionError::VotingNotOpen);
        }
        if ledger > competition.voting_end_ledger {
            return Err(CompetitionError::VotingClosed);
        }
        if voter == entrant {
            return Err(CompetitionError::SelfVoteNotAllowed);
        }

        let weight = reputation_of(&env, &voter);
        if weight < competition.rules.min_reputation || weight == 0 {
            return Err(CompetitionError::ReputationTooLow);
        }

        let voted_key = DataKey::Voted(competition_id.clone(), voter.clone());
        if env.storage().persistent().has(&voted_key) {
            return Err(CompetitionError::AlreadyVoted);
        }

        let mut submission = load_submission(&env, &competition_id, &entrant)?;
        env.storage().persistent().set(&voted_key, &entrant);

        submission.votes += weight as i128;
        submission.voter_count += 1;
        env.storage().persistent().set(
            &DataKey::Submission(competition_id.clone(), entrant.clone()),
            &submission,
        );

        competition.total_votes += weight as i128;
        save_competition(&env, &competition);

        env.events().publish(
            (symbol_short!("vote"),),
            (competition_id, voter, entrant, weight),
        );
        Ok(weight)
    }

    /// Close voting and fix the ranking. Permissionless once the voting window
    /// has passed, so a competition cannot be held open by an absent organizer.
    pub fn finalize(env: Env, competition_id: Bytes) -> Result<Vec<Winner>, CompetitionError> {
        require_initialized(&env)?;
        let mut competition = load_competition(&env, &competition_id)?;
        if competition.status != CompetitionStatus::Open {
            return Err(CompetitionError::AlreadyFinalized);
        }
        if env.ledger().sequence() <= competition.voting_end_ledger {
            return Err(CompetitionError::VotingClosed);
        }

        let ranked = rank_entries(&env, &competition_id);
        let positions = if ranked.len() < competition.rules.prize_split_bps.len() {
            ranked.len()
        } else {
            competition.rules.prize_split_bps.len()
        };

        let mut winners: Vec<Winner> = Vec::new(&env);
        let mut allocated: i128 = 0;
        for i in 0..positions {
            let submission = ranked.get(i).unwrap();
            let share = competition.rules.prize_split_bps.get(i).unwrap();
            let prize = competition
                .prize_pool
                .checked_mul(share as i128)
                .ok_or(CompetitionError::ArithmeticOverflow)?
                / TOTAL_BPS as i128;
            allocated += prize;
            winners.push_back(Winner {
                rank: i + 1,
                entrant: submission.entrant.clone(),
                votes: submission.votes,
                prize,
            });
        }
        // Rounding dust joins the top prize when every position was filled, so
        // the pool is fully allocated. If positions went unfilled the remainder
        // is left for the organizer refund in `distribute_prizes`.
        if !winners.is_empty() && positions == competition.rules.prize_split_bps.len() {
            let mut top = winners.get(0).unwrap();
            top.prize += competition.prize_pool - allocated;
            winners.set(0, top);
        }

        env.storage()
            .persistent()
            .set(&DataKey::Winners(competition_id.clone()), &winners);
        competition.status = CompetitionStatus::Finalized;
        save_competition(&env, &competition);

        let history_key = DataKey::History;
        let mut history: Vec<CompetitionSummary> = env
            .storage()
            .persistent()
            .get(&history_key)
            .unwrap_or_else(|| Vec::new(&env));
        let limit: u32 = env
            .storage()
            .instance()
            .get(&DataKey::HistoryLimit)
            .unwrap_or(0);
        while history.len() >= limit {
            history.pop_front();
        }
        history.push_back(CompetitionSummary {
            competition_id: competition_id.clone(),
            organizer: competition.organizer.clone(),
            prize_pool: competition.prize_pool,
            submission_count: competition.submission_count,
            total_votes: competition.total_votes,
            top_entrant: winners.get(0).map(|w| w.entrant),
            finalized_ledger: env.ledger().sequence(),
        });
        env.storage().persistent().set(&history_key, &history);

        env.events()
            .publish((symbol_short!("final"),), (competition_id, winners.len()));
        Ok(winners)
    }

    /// Pay the ranked winners. Anything not awarded — because fewer entries
    /// arrived than there were prize positions — goes back to the organizer.
    pub fn distribute_prizes(env: Env, competition_id: Bytes) -> Result<(), CompetitionError> {
        require_initialized(&env)?;
        let mut competition = load_competition(&env, &competition_id)?;
        match competition.status {
            CompetitionStatus::Open => return Err(CompetitionError::NotFinalized),
            CompetitionStatus::Settled => return Err(CompetitionError::AlreadySettled),
            CompetitionStatus::Finalized => {}
        }

        let winners: Vec<Winner> = env
            .storage()
            .persistent()
            .get(&DataKey::Winners(competition_id.clone()))
            .unwrap_or_else(|| Vec::new(&env));

        competition.status = CompetitionStatus::Settled;
        save_competition(&env, &competition);

        let token_client = token::Client::new(&env, &competition.token);
        let mut paid: i128 = 0;
        for winner in winners.iter() {
            if winner.prize > 0 {
                token_client.transfer(
                    &env.current_contract_address(),
                    &winner.entrant,
                    &winner.prize,
                );
                paid += winner.prize;
            }
        }
        let unawarded = competition.prize_pool - paid;
        if unawarded > 0 {
            token_client.transfer(
                &env.current_contract_address(),
                &competition.organizer,
                &unawarded,
            );
        }

        env.events().publish(
            (symbol_short!("prizes"),),
            (competition_id, paid, unawarded),
        );
        Ok(())
    }

    pub fn get_competition(
        env: Env,
        competition_id: Bytes,
    ) -> Result<Competition, CompetitionError> {
        load_competition(&env, &competition_id)
    }

    pub fn get_submission(
        env: Env,
        competition_id: Bytes,
        entrant: Address,
    ) -> Result<Submission, CompetitionError> {
        load_submission(&env, &competition_id, &entrant)
    }

    pub fn get_entrants(env: Env, competition_id: Bytes) -> Vec<Address> {
        entrants_of(&env, &competition_id)
    }

    pub fn get_winners(env: Env, competition_id: Bytes) -> Vec<Winner> {
        env.storage()
            .persistent()
            .get(&DataKey::Winners(competition_id))
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_history(env: Env) -> Vec<CompetitionSummary> {
        env.storage()
            .persistent()
            .get(&DataKey::History)
            .unwrap_or_else(|| Vec::new(&env))
    }
}
