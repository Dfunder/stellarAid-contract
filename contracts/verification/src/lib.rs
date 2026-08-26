#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, String, Vec};

pub mod errors;
pub mod types;

#[cfg(test)]
mod test;

use errors::VerificationError;
use types::{DataKey, Portfolio, PortfolioStatus, QualityScore, ReviewOutcome, VerificationRecord};

/// Weights applied to each quality criterion; they sum to 100 so the blended
/// score stays on the same 0..=100 scale as the individual marks.
const WEIGHT_ORIGINALITY: u32 = 30;
const WEIGHT_TECHNIQUE: u32 = 30;
const WEIGHT_CONSISTENCY: u32 = 20;
const WEIGHT_PRESENTATION: u32 = 20;

#[contract]
pub struct Verification;

fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

fn get_admin(env: &Env) -> Result<Address, VerificationError> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(VerificationError::NotInitialized)
}

fn require_admin(env: &Env) -> Result<Address, VerificationError> {
    let admin = get_admin(env)?;
    admin.require_auth();
    Ok(admin)
}

fn require_reviewer(env: &Env, reviewer: &Address) -> Result<(), VerificationError> {
    if !has_admin(env) {
        return Err(VerificationError::NotInitialized);
    }
    let is_admin = get_admin(env)? == *reviewer;
    if !is_admin
        && !env
            .storage()
            .instance()
            .has(&DataKey::Reviewer(reviewer.clone()))
    {
        return Err(VerificationError::Unauthorized);
    }
    reviewer.require_auth();
    Ok(())
}

fn get_u32(env: &Env, key: &DataKey) -> u32 {
    env.storage().instance().get(key).unwrap_or(0)
}

fn load_portfolio(env: &Env, artist: &Address) -> Result<Portfolio, VerificationError> {
    env.storage()
        .persistent()
        .get(&DataKey::Portfolio(artist.clone()))
        .ok_or(VerificationError::PortfolioNotFound)
}

fn save_portfolio(env: &Env, portfolio: &Portfolio) {
    env.storage()
        .persistent()
        .set(&DataKey::Portfolio(portfolio.artist.clone()), portfolio);
}

fn push_history(env: &Env, artist: &Address, record: VerificationRecord) {
    let key = DataKey::History(artist.clone());
    let mut history: Vec<VerificationRecord> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));
    let limit = get_u32(env, &DataKey::HistoryLimit);
    while history.len() >= limit {
        history.pop_front();
    }
    history.push_back(record);
    env.storage().persistent().set(&key, &history);
}

fn overall_score(quality: &QualityScore) -> Result<u32, VerificationError> {
    for mark in [
        quality.originality,
        quality.technique,
        quality.consistency,
        quality.presentation,
    ] {
        if mark > 100 {
            return Err(VerificationError::InvalidScore);
        }
    }
    Ok((quality.originality * WEIGHT_ORIGINALITY
        + quality.technique * WEIGHT_TECHNIQUE
        + quality.consistency * WEIGHT_CONSISTENCY
        + quality.presentation * WEIGHT_PRESENTATION)
        / 100)
}

/// A verified portfolio goes stale once its refresh deadline passes; the stored
/// status only flips when someone calls `flag_update_required`, so freshness is
/// derived here rather than read straight off the record.
fn is_stale(env: &Env, portfolio: &Portfolio) -> bool {
    portfolio.status == PortfolioStatus::Verified
        && portfolio.next_update_ledger > 0
        && env.ledger().sequence() > portfolio.next_update_ledger
}

#[contractimpl]
impl Verification {
    pub fn initialize(
        env: Env,
        admin: Address,
        min_score: u32,
        min_work_count: u32,
        update_interval: u32,
        history_limit: u32,
    ) -> Result<(), VerificationError> {
        if has_admin(&env) {
            return Err(VerificationError::AlreadyInitialized);
        }
        if min_score > 100 {
            return Err(VerificationError::InvalidScore);
        }
        if update_interval == 0 || history_limit == 0 {
            return Err(VerificationError::InvalidInterval);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::MinScore, &min_score);
        env.storage()
            .instance()
            .set(&DataKey::MinWorkCount, &min_work_count);
        env.storage()
            .instance()
            .set(&DataKey::UpdateInterval, &update_interval);
        env.storage()
            .instance()
            .set(&DataKey::HistoryLimit, &history_limit);
        env.events().publish(
            (symbol_short!("init"),),
            (admin, min_score, min_work_count, update_interval),
        );
        Ok(())
    }

    pub fn add_reviewer(env: Env, reviewer: Address) -> Result<(), VerificationError> {
        require_admin(&env)?;
        env.storage()
            .instance()
            .set(&DataKey::Reviewer(reviewer.clone()), &true);
        env.events().publish((symbol_short!("rev_add"),), reviewer);
        Ok(())
    }

    pub fn remove_reviewer(env: Env, reviewer: Address) -> Result<(), VerificationError> {
        require_admin(&env)?;
        env.storage()
            .instance()
            .remove(&DataKey::Reviewer(reviewer.clone()));
        env.events().publish((symbol_short!("rev_rm"),), reviewer);
        Ok(())
    }

    pub fn is_reviewer(env: Env, reviewer: Address) -> bool {
        env.storage().instance().has(&DataKey::Reviewer(reviewer))
    }

    /// First-time portfolio submission. The artist owns the record, so only the
    /// artist can create or later revise it.
    pub fn submit_portfolio(
        env: Env,
        artist: Address,
        metadata_uri: String,
        work_count: u32,
    ) -> Result<(), VerificationError> {
        if !has_admin(&env) {
            return Err(VerificationError::NotInitialized);
        }
        artist.require_auth();
        if env
            .storage()
            .persistent()
            .has(&DataKey::Portfolio(artist.clone()))
        {
            return Err(VerificationError::PortfolioExists);
        }
        if work_count < get_u32(&env, &DataKey::MinWorkCount) {
            return Err(VerificationError::InvalidWorkCount);
        }
        let portfolio = Portfolio {
            artist: artist.clone(),
            metadata_uri,
            work_count,
            status: PortfolioStatus::Submitted,
            score: 0,
            revision: 1,
            submitted_ledger: env.ledger().sequence(),
            reviewed_ledger: 0,
            reviewer: None,
            next_update_ledger: 0,
        };
        save_portfolio(&env, &portfolio);
        env.events()
            .publish((symbol_short!("submitted"),), (artist, work_count));
        Ok(())
    }

    /// Revise an existing portfolio. Any revision invalidates the current
    /// verdict and sends the portfolio back through review.
    pub fn update_portfolio(
        env: Env,
        artist: Address,
        metadata_uri: String,
        work_count: u32,
    ) -> Result<(), VerificationError> {
        if !has_admin(&env) {
            return Err(VerificationError::NotInitialized);
        }
        artist.require_auth();
        let mut portfolio = load_portfolio(&env, &artist)?;
        if work_count < get_u32(&env, &DataKey::MinWorkCount) {
            return Err(VerificationError::InvalidWorkCount);
        }
        portfolio.metadata_uri = metadata_uri;
        portfolio.work_count = work_count;
        portfolio.status = PortfolioStatus::Submitted;
        portfolio.score = 0;
        portfolio.revision += 1;
        portfolio.submitted_ledger = env.ledger().sequence();
        portfolio.reviewed_ledger = 0;
        portfolio.reviewer = None;
        portfolio.next_update_ledger = 0;
        save_portfolio(&env, &portfolio);
        push_history(
            &env,
            &artist,
            VerificationRecord {
                revision: portfolio.revision,
                outcome: ReviewOutcome::Resubmitted,
                score: 0,
                quality: QualityScore {
                    originality: 0,
                    technique: 0,
                    consistency: 0,
                    presentation: 0,
                },
                reviewer: None,
                ledger: env.ledger().sequence(),
                note: String::from_str(&env, ""),
            },
        );
        env.events()
            .publish((symbol_short!("updated"),), (artist, portfolio.revision));
        Ok(())
    }

    /// Claim a submitted portfolio for manual review, so two reviewers do not
    /// pick up the same queue entry.
    pub fn start_review(
        env: Env,
        reviewer: Address,
        artist: Address,
    ) -> Result<(), VerificationError> {
        require_reviewer(&env, &reviewer)?;
        let mut portfolio = load_portfolio(&env, &artist)?;
        if portfolio.status != PortfolioStatus::Submitted {
            return Err(VerificationError::InvalidStatus);
        }
        portfolio.status = PortfolioStatus::UnderReview;
        portfolio.reviewer = Some(reviewer.clone());
        save_portfolio(&env, &portfolio);
        env.events()
            .publish((symbol_short!("review"),), (artist, reviewer));
        Ok(())
    }

    /// Record a manual verdict. The blended quality score decides approval, and
    /// an approval starts the clock on the next required portfolio update.
    pub fn review_portfolio(
        env: Env,
        reviewer: Address,
        artist: Address,
        quality: QualityScore,
        note: String,
    ) -> Result<u32, VerificationError> {
        require_reviewer(&env, &reviewer)?;
        let mut portfolio = load_portfolio(&env, &artist)?;
        if portfolio.status != PortfolioStatus::UnderReview {
            return Err(VerificationError::InvalidStatus);
        }
        let score = overall_score(&quality)?;
        let approved = score >= get_u32(&env, &DataKey::MinScore);
        let ledger = env.ledger().sequence();
        portfolio.score = score;
        portfolio.reviewed_ledger = ledger;
        portfolio.reviewer = Some(reviewer.clone());
        if approved {
            portfolio.status = PortfolioStatus::Verified;
            portfolio.next_update_ledger = ledger + get_u32(&env, &DataKey::UpdateInterval);
        } else {
            portfolio.status = PortfolioStatus::Rejected;
            portfolio.next_update_ledger = 0;
        }
        save_portfolio(&env, &portfolio);
        push_history(
            &env,
            &artist,
            VerificationRecord {
                revision: portfolio.revision,
                outcome: if approved {
                    ReviewOutcome::Approved
                } else {
                    ReviewOutcome::Rejected
                },
                score,
                quality,
                reviewer: Some(reviewer.clone()),
                ledger,
                note,
            },
        );
        env.events().publish(
            (
                symbol_short!("reviewed"),
                if approved {
                    symbol_short!("approved")
                } else {
                    symbol_short!("rejected")
                },
            ),
            (artist, reviewer, score),
        );
        Ok(score)
    }

    /// Permissionless: move a verified-but-stale portfolio into
    /// `UpdateRequired` so downstream badge checks stop passing.
    pub fn flag_update_required(env: Env, artist: Address) -> Result<(), VerificationError> {
        if !has_admin(&env) {
            return Err(VerificationError::NotInitialized);
        }
        let mut portfolio = load_portfolio(&env, &artist)?;
        if !is_stale(&env, &portfolio) {
            return Err(VerificationError::UpdateNotDue);
        }
        portfolio.status = PortfolioStatus::UpdateRequired;
        save_portfolio(&env, &portfolio);
        env.events().publish((symbol_short!("stale"),), artist);
        Ok(())
    }

    pub fn get_portfolio(env: Env, artist: Address) -> Result<Portfolio, VerificationError> {
        load_portfolio(&env, &artist)
    }

    pub fn get_history(env: Env, artist: Address) -> Vec<VerificationRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::History(artist))
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Badge eligibility: approved and not past its refresh deadline.
    pub fn is_verified(env: Env, artist: Address) -> bool {
        match load_portfolio(&env, &artist) {
            Ok(portfolio) => {
                portfolio.status == PortfolioStatus::Verified && !is_stale(&env, &portfolio)
            }
            Err(_) => false,
        }
    }

    pub fn requires_update(env: Env, artist: Address) -> bool {
        match load_portfolio(&env, &artist) {
            Ok(portfolio) => {
                portfolio.status == PortfolioStatus::UpdateRequired || is_stale(&env, &portfolio)
            }
            Err(_) => false,
        }
    }

    pub fn set_min_score(env: Env, min_score: u32) -> Result<(), VerificationError> {
        require_admin(&env)?;
        if min_score > 100 {
            return Err(VerificationError::InvalidScore);
        }
        env.storage().instance().set(&DataKey::MinScore, &min_score);
        Ok(())
    }

    pub fn set_update_interval(env: Env, update_interval: u32) -> Result<(), VerificationError> {
        require_admin(&env)?;
        if update_interval == 0 {
            return Err(VerificationError::InvalidInterval);
        }
        env.storage()
            .instance()
            .set(&DataKey::UpdateInterval, &update_interval);
        Ok(())
    }
}
