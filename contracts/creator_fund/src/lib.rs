#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, token, Address, Bytes, Env, String, Vec};

pub mod errors;
pub mod types;

#[cfg(test)]
mod test;

use errors::FundError;
use types::{
    Allocation, DataKey, DistributionRule, Fund, FundType, GrowthPoint, Proposal, ProposalStatus,
    TOTAL_BPS,
};

#[contract]
pub struct CreatorFund;

fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

fn require_initialized(env: &Env) -> Result<(), FundError> {
    if has_admin(env) {
        Ok(())
    } else {
        Err(FundError::NotInitialized)
    }
}

fn load_fund(env: &Env, fund_id: &Bytes) -> Result<Fund, FundError> {
    env.storage()
        .persistent()
        .get(&DataKey::Fund(fund_id.clone()))
        .ok_or(FundError::FundNotFound)
}

fn save_fund(env: &Env, fund: &Fund) {
    env.storage()
        .persistent()
        .set(&DataKey::Fund(fund.id.clone()), fund);
}

fn load_proposal(env: &Env, proposal_id: &Bytes) -> Result<Proposal, FundError> {
    env.storage()
        .persistent()
        .get(&DataKey::Proposal(proposal_id.clone()))
        .ok_or(FundError::ProposalNotFound)
}

fn save_proposal(env: &Env, proposal: &Proposal) {
    env.storage()
        .persistent()
        .set(&DataKey::Proposal(proposal.id.clone()), proposal);
}

fn validate_rule(rule: &DistributionRule) -> Result<(), FundError> {
    if rule.max_allocation_bps == 0
        || rule.max_allocation_bps > TOTAL_BPS
        || rule.quorum_bps > TOTAL_BPS
        || rule.voting_ledgers == 0
        || rule.min_reserve < 0
    {
        return Err(FundError::InvalidRule);
    }
    Ok(())
}

fn history_limit(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::HistoryLimit)
        .unwrap_or(0)
}

fn record_growth(env: &Env, fund: &Fund) {
    let key = DataKey::Growth(fund.id.clone());
    let mut growth: Vec<GrowthPoint> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));
    let limit = history_limit(env);
    while growth.len() >= limit {
        growth.pop_front();
    }
    growth.push_back(GrowthPoint {
        ledger: env.ledger().sequence(),
        balance: fund.balance,
        total_contributed: fund.total_contributed,
        total_allocated: fund.total_allocated,
    });
    env.storage().persistent().set(&key, &growth);
}

fn voting_power(env: &Env, fund_id: &Bytes, account: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Contribution(fund_id.clone(), account.clone()))
        .unwrap_or(0)
}

/// A payout must fit under the per-allocation cap and leave the reserve intact.
fn check_distribution_rule(fund: &Fund, amount: i128) -> Result<(), FundError> {
    let cap = fund
        .balance
        .checked_mul(fund.rule.max_allocation_bps as i128)
        .ok_or(FundError::ArithmeticOverflow)?
        / TOTAL_BPS as i128;
    if amount > cap {
        return Err(FundError::ExceedsAllocationLimit);
    }
    if fund.balance - amount < fund.rule.min_reserve {
        return Err(FundError::ReserveBreached);
    }
    Ok(())
}

#[contractimpl]
impl CreatorFund {
    pub fn initialize(env: Env, admin: Address, history_limit: u32) -> Result<(), FundError> {
        if has_admin(&env) {
            return Err(FundError::AlreadyInitialized);
        }
        if history_limit == 0 {
            return Err(FundError::InvalidAmount);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::HistoryLimit, &history_limit);
        env.events()
            .publish((symbol_short!("init"),), (admin, history_limit));
        Ok(())
    }

    /// Open a new pool. The steward owns the fund's configuration; spending
    /// still goes through contributor voting.
    pub fn create_fund(
        env: Env,
        fund_id: Bytes,
        fund_type: FundType,
        steward: Address,
        token: Address,
        rule: DistributionRule,
    ) -> Result<(), FundError> {
        require_initialized(&env)?;
        steward.require_auth();
        if env
            .storage()
            .persistent()
            .has(&DataKey::Fund(fund_id.clone()))
        {
            return Err(FundError::FundExists);
        }
        validate_rule(&rule)?;
        let fund = Fund {
            id: fund_id.clone(),
            fund_type,
            steward: steward.clone(),
            token,
            balance: 0,
            total_contributed: 0,
            total_allocated: 0,
            contributor_count: 0,
            rule,
            created_ledger: env.ledger().sequence(),
        };
        save_fund(&env, &fund);
        record_growth(&env, &fund);
        env.events()
            .publish((symbol_short!("fund_new"),), (fund_id, fund_type, steward));
        Ok(())
    }

    pub fn set_rule(env: Env, fund_id: Bytes, rule: DistributionRule) -> Result<(), FundError> {
        require_initialized(&env)?;
        let mut fund = load_fund(&env, &fund_id)?;
        fund.steward.require_auth();
        validate_rule(&rule)?;
        fund.rule = rule;
        save_fund(&env, &fund);
        env.events().publish((symbol_short!("rule"),), fund_id);
        Ok(())
    }

    /// Contribute capital to a fund. Contributions are the source of voting
    /// power, so they are tracked per account as well as in aggregate.
    pub fn contribute(
        env: Env,
        fund_id: Bytes,
        contributor: Address,
        amount: i128,
    ) -> Result<(), FundError> {
        require_initialized(&env)?;
        contributor.require_auth();
        if amount <= 0 {
            return Err(FundError::InvalidAmount);
        }
        let mut fund = load_fund(&env, &fund_id)?;

        let key = DataKey::Contribution(fund_id.clone(), contributor.clone());
        let previous: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        if previous == 0 {
            fund.contributor_count += 1;
        }
        env.storage().persistent().set(&key, &(previous + amount));

        fund.balance = fund
            .balance
            .checked_add(amount)
            .ok_or(FundError::ArithmeticOverflow)?;
        fund.total_contributed = fund
            .total_contributed
            .checked_add(amount)
            .ok_or(FundError::ArithmeticOverflow)?;
        save_fund(&env, &fund);
        record_growth(&env, &fund);

        token::Client::new(&env, &fund.token).transfer(
            &contributor,
            &env.current_contract_address(),
            &amount,
        );

        env.events()
            .publish((symbol_short!("contrib"),), (fund_id, contributor, amount));
        Ok(())
    }

    /// Propose an allocation out of the fund. Only contributors can propose,
    /// and the request must already satisfy the fund's distribution rule.
    pub fn propose_allocation(
        env: Env,
        proposal_id: Bytes,
        fund_id: Bytes,
        proposer: Address,
        recipient: Address,
        amount: i128,
        memo: String,
    ) -> Result<(), FundError> {
        require_initialized(&env)?;
        proposer.require_auth();
        if env
            .storage()
            .persistent()
            .has(&DataKey::Proposal(proposal_id.clone()))
        {
            return Err(FundError::ProposalExists);
        }
        let fund = load_fund(&env, &fund_id)?;
        if amount <= 0 {
            return Err(FundError::InvalidAmount);
        }
        if voting_power(&env, &fund_id, &proposer) == 0 {
            return Err(FundError::NotContributor);
        }
        check_distribution_rule(&fund, amount)?;

        let ledger = env.ledger().sequence();
        let proposal = Proposal {
            id: proposal_id.clone(),
            fund_id: fund_id.clone(),
            proposer: proposer.clone(),
            recipient,
            amount,
            status: ProposalStatus::Voting,
            votes_for: 0,
            votes_against: 0,
            created_ledger: ledger,
            voting_ends_ledger: ledger + fund.rule.voting_ledgers,
            memo,
        };
        save_proposal(&env, &proposal);
        env.events()
            .publish((symbol_short!("proposed"),), (proposal_id, fund_id, amount));
        Ok(())
    }

    /// One vote per contributor per proposal, weighted by contributed capital.
    pub fn vote(
        env: Env,
        proposal_id: Bytes,
        voter: Address,
        support: bool,
    ) -> Result<i128, FundError> {
        require_initialized(&env)?;
        voter.require_auth();
        let mut proposal = load_proposal(&env, &proposal_id)?;
        if proposal.status != ProposalStatus::Voting {
            return Err(FundError::VotingClosed);
        }
        if env.ledger().sequence() > proposal.voting_ends_ledger {
            return Err(FundError::VotingClosed);
        }
        let power = voting_power(&env, &proposal.fund_id, &voter);
        if power == 0 {
            return Err(FundError::NotContributor);
        }
        let voted_key = DataKey::Voted(proposal_id.clone(), voter.clone());
        if env.storage().persistent().has(&voted_key) {
            return Err(FundError::AlreadyVoted);
        }
        env.storage().persistent().set(&voted_key, &support);

        if support {
            proposal.votes_for += power;
        } else {
            proposal.votes_against += power;
        }
        save_proposal(&env, &proposal);
        env.events().publish(
            (symbol_short!("vote"),),
            (proposal_id, voter, support, power),
        );
        Ok(power)
    }

    /// Close voting and settle the outcome. Quorum is measured against the
    /// capital contributed to the fund, not the number of voters.
    pub fn finalize_proposal(env: Env, proposal_id: Bytes) -> Result<ProposalStatus, FundError> {
        require_initialized(&env)?;
        let mut proposal = load_proposal(&env, &proposal_id)?;
        if proposal.status != ProposalStatus::Voting {
            return Err(FundError::VotingClosed);
        }
        if env.ledger().sequence() <= proposal.voting_ends_ledger {
            return Err(FundError::VotingOpen);
        }
        let fund = load_fund(&env, &proposal.fund_id)?;
        let turnout = proposal.votes_for + proposal.votes_against;
        let quorum = fund
            .total_contributed
            .checked_mul(fund.rule.quorum_bps as i128)
            .ok_or(FundError::ArithmeticOverflow)?
            / TOTAL_BPS as i128;
        proposal.status = if turnout >= quorum && proposal.votes_for > proposal.votes_against {
            ProposalStatus::Approved
        } else {
            ProposalStatus::Rejected
        };
        save_proposal(&env, &proposal);
        env.events()
            .publish((symbol_short!("finalize"),), (proposal_id, proposal.status));
        Ok(proposal.status)
    }

    /// Pay out an approved allocation. The distribution rule is re-checked
    /// against the balance at execution time, which may have moved since the
    /// proposal was raised.
    pub fn execute_allocation(env: Env, proposal_id: Bytes) -> Result<(), FundError> {
        require_initialized(&env)?;
        let mut proposal = load_proposal(&env, &proposal_id)?;
        if proposal.status != ProposalStatus::Approved {
            return Err(FundError::ProposalNotApproved);
        }
        let mut fund = load_fund(&env, &proposal.fund_id)?;
        check_distribution_rule(&fund, proposal.amount)?;

        fund.balance -= proposal.amount;
        fund.total_allocated = fund
            .total_allocated
            .checked_add(proposal.amount)
            .ok_or(FundError::ArithmeticOverflow)?;
        proposal.status = ProposalStatus::Executed;
        save_proposal(&env, &proposal);
        save_fund(&env, &fund);
        record_growth(&env, &fund);

        let allocations_key = DataKey::Allocations(fund.id.clone());
        let mut allocations: Vec<Allocation> = env
            .storage()
            .persistent()
            .get(&allocations_key)
            .unwrap_or_else(|| Vec::new(&env));
        let limit = history_limit(&env);
        while allocations.len() >= limit {
            allocations.pop_front();
        }
        allocations.push_back(Allocation {
            proposal_id: proposal_id.clone(),
            recipient: proposal.recipient.clone(),
            amount: proposal.amount,
            ledger: env.ledger().sequence(),
        });
        env.storage()
            .persistent()
            .set(&allocations_key, &allocations);

        token::Client::new(&env, &fund.token).transfer(
            &env.current_contract_address(),
            &proposal.recipient,
            &proposal.amount,
        );

        env.events().publish(
            (symbol_short!("executed"),),
            (proposal_id, proposal.recipient, proposal.amount),
        );
        Ok(())
    }

    pub fn get_fund(env: Env, fund_id: Bytes) -> Result<Fund, FundError> {
        load_fund(&env, &fund_id)
    }

    pub fn get_proposal(env: Env, proposal_id: Bytes) -> Result<Proposal, FundError> {
        load_proposal(&env, &proposal_id)
    }

    pub fn get_contribution(env: Env, fund_id: Bytes, account: Address) -> i128 {
        voting_power(&env, &fund_id, &account)
    }

    pub fn get_allocations(env: Env, fund_id: Bytes) -> Vec<Allocation> {
        env.storage()
            .persistent()
            .get(&DataKey::Allocations(fund_id))
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_growth(env: Env, fund_id: Bytes) -> Vec<GrowthPoint> {
        env.storage()
            .persistent()
            .get(&DataKey::Growth(fund_id))
            .unwrap_or_else(|| Vec::new(&env))
    }
}
