//! Portfolio Analytics Contract — closes #602.
//!
//! Tracks artist performance metrics:
//! - Earnings by category / client
//! - Project completion rate
//! - Response time analytics
//! - Client satisfaction trends
//! - Earnings predictions
//!
//! Designed to be updated by trusted platform services (admin) whenever a
//! project is completed, cancelled, or a new commission is recorded.

#![no_std]

mod errors;
mod types;

#[cfg(test)]
mod test;

use soroban_sdk::{
    contract, contractimpl, symbol_short, Address, Bytes, Env, String, Vec,
};
use errors::AnalyticsError;
use types::{
    ArtistStats, CategoryEarnings, CompletionStats, DataKey, EarningsPrediction,
    ResponseTimeStats, SatisfactionDataPoint,
};

/// Approximate ledgers per month (~30 days at 6 s/ledger).
const LEDGERS_PER_MONTH: u32 = 432_000;

/// Maximum allowed length for a category string (bytes).
const MAX_CATEGORY_LEN: usize = 64;

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn require_admin(env: &Env) -> Result<(), AnalyticsError> {
    if !env.storage().instance().has(&DataKey::Admin) {
        return Err(AnalyticsError::NotInitialized);
    }
    let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    admin.require_auth();
    Ok(())
}

fn load_stats(env: &Env, artist: &Address) -> ArtistStats {
    env.storage()
        .persistent()
        .get(&DataKey::ArtistStats(artist.clone()))
        .unwrap_or(ArtistStats {
            total_earnings: 0,
            projects_started: 0,
            projects_completed: 0,
            projects_cancelled: 0,
            total_response_time_ledgers: 0,
            response_time_samples: 0,
            total_satisfaction: 0,
            satisfaction_count: 0,
            last_updated_ledger: 0,
        })
}

fn save_stats(env: &Env, artist: &Address, stats: &ArtistStats) {
    env.storage()
        .persistent()
        .set(&DataKey::ArtistStats(artist.clone()), stats);
}

fn compute_completion_rate(started: u32, completed: u32) -> u32 {
    if started == 0 {
        return 0;
    }
    (completed as u64 * 10_000 / started as u64) as u32
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct AnalyticsContract;

#[contractimpl]
impl AnalyticsContract {
    // -----------------------------------------------------------------------
    // Initialization
    // -----------------------------------------------------------------------

    /// Initialize the contract with an admin address.
    ///
    /// Can only be called once.
    pub fn initialize(env: Env, admin: Address) -> Result<(), AnalyticsError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(AnalyticsError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.events().publish((symbol_short!("ana_init"),), (admin,));
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Project lifecycle recording
    // -----------------------------------------------------------------------

    /// Record a new project being started for an artist.
    ///
    /// Admin only. Increments `projects_started`.
    ///
    /// Closes #602.
    pub fn record_project_started(
        env: Env,
        artist: Address,
    ) -> Result<(), AnalyticsError> {
        require_admin(&env)?;

        let mut stats = load_stats(&env, &artist);
        stats.projects_started = stats
            .projects_started
            .checked_add(1)
            .ok_or(AnalyticsError::ArithmeticOverflow)?;
        stats.last_updated_ledger = env.ledger().sequence();
        save_stats(&env, &artist, &stats);

        env.events()
            .publish((symbol_short!("prj_strt"),), (artist,));
        Ok(())
    }

    /// Record a completed project with earnings and optional category tag.
    ///
    /// Admin only. Updates earnings totals (overall + per-category) and
    /// completion counts.
    ///
    /// Closes #602.
    pub fn record_project_completed(
        env: Env,
        artist: Address,
        amount_usdc: i128,
        category: String,
        _commission_id: Bytes,
    ) -> Result<(), AnalyticsError> {
        require_admin(&env)?;

        if amount_usdc < 0 {
            return Err(AnalyticsError::InvalidValue);
        }
        if category.len() as usize > MAX_CATEGORY_LEN {
            return Err(AnalyticsError::CategoryTooLong);
        }

        let mut stats = load_stats(&env, &artist);
        stats.total_earnings = stats
            .total_earnings
            .checked_add(amount_usdc)
            .ok_or(AnalyticsError::ArithmeticOverflow)?;
        stats.projects_completed = stats
            .projects_completed
            .checked_add(1)
            .ok_or(AnalyticsError::ArithmeticOverflow)?;
        stats.last_updated_ledger = env.ledger().sequence();
        save_stats(&env, &artist, &stats);

        // Update per-category earnings.
        let cat_key = DataKey::CategoryEarnings(artist.clone(), category.clone());
        let mut cat: CategoryEarnings = env
            .storage()
            .persistent()
            .get(&cat_key)
            .unwrap_or(CategoryEarnings {
                category: category.clone(),
                earnings: 0,
                project_count: 0,
            });
        cat.earnings = cat
            .earnings
            .checked_add(amount_usdc)
            .ok_or(AnalyticsError::ArithmeticOverflow)?;
        cat.project_count = cat
            .project_count
            .checked_add(1)
            .ok_or(AnalyticsError::ArithmeticOverflow)?;
        env.storage().persistent().set(&cat_key, &cat);

        // Track category tag list.
        let mut cats: Vec<String> = env
            .storage()
            .persistent()
            .get(&DataKey::ArtistCategories(artist.clone()))
            .unwrap_or(Vec::new(&env));
        let already_tracked = cats.iter().any(|c| c == category);
        if !already_tracked {
            cats.push_back(category.clone());
            env.storage()
                .persistent()
                .set(&DataKey::ArtistCategories(artist.clone()), &cats);
        }

        env.events().publish(
            (symbol_short!("prj_done"),),
            (artist, amount_usdc, category),
        );
        Ok(())
    }

    /// Record a cancelled / abandoned project.
    ///
    /// Admin only.
    ///
    /// Closes #602.
    pub fn record_project_cancelled(
        env: Env,
        artist: Address,
    ) -> Result<(), AnalyticsError> {
        require_admin(&env)?;

        let mut stats = load_stats(&env, &artist);
        stats.projects_cancelled = stats
            .projects_cancelled
            .checked_add(1)
            .ok_or(AnalyticsError::ArithmeticOverflow)?;
        stats.last_updated_ledger = env.ledger().sequence();
        save_stats(&env, &artist, &stats);

        env.events()
            .publish((symbol_short!("prj_canc"),), (artist,));
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Response time analytics
    // -----------------------------------------------------------------------

    /// Record a response time sample (in ledgers) for an artist.
    ///
    /// Admin only.
    ///
    /// Closes #602.
    pub fn record_response_time(
        env: Env,
        artist: Address,
        response_time_ledgers: u32,
    ) -> Result<(), AnalyticsError> {
        require_admin(&env)?;

        let mut stats = load_stats(&env, &artist);
        stats.total_response_time_ledgers = stats
            .total_response_time_ledgers
            .checked_add(response_time_ledgers as u64)
            .ok_or(AnalyticsError::ArithmeticOverflow)?;
        stats.response_time_samples = stats
            .response_time_samples
            .checked_add(1)
            .ok_or(AnalyticsError::ArithmeticOverflow)?;
        stats.last_updated_ledger = env.ledger().sequence();
        save_stats(&env, &artist, &stats);

        env.events().publish(
            (symbol_short!("resp_t"),),
            (artist, response_time_ledgers),
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Client satisfaction trends
    // -----------------------------------------------------------------------

    /// Record a client satisfaction rating for an artist.
    ///
    /// - `rating` must be in `[1, 100]`.
    /// - Admin only.
    ///
    /// Closes #602.
    pub fn record_satisfaction(
        env: Env,
        artist: Address,
        commission_id: Bytes,
        rating: u32,
    ) -> Result<(), AnalyticsError> {
        require_admin(&env)?;

        if rating < 1 || rating > 100 {
            return Err(AnalyticsError::InvalidValue);
        }

        let mut stats = load_stats(&env, &artist);
        stats.total_satisfaction = stats
            .total_satisfaction
            .checked_add(rating as u64)
            .ok_or(AnalyticsError::ArithmeticOverflow)?;
        stats.satisfaction_count = stats
            .satisfaction_count
            .checked_add(1)
            .ok_or(AnalyticsError::ArithmeticOverflow)?;
        stats.last_updated_ledger = env.ledger().sequence();
        save_stats(&env, &artist, &stats);

        // Append to trend log.
        let mut trend: Vec<SatisfactionDataPoint> = env
            .storage()
            .persistent()
            .get(&DataKey::SatisfactionTrend(artist.clone()))
            .unwrap_or(Vec::new(&env));
        trend.push_back(SatisfactionDataPoint {
            commission_id,
            rating,
            recorded_ledger: env.ledger().sequence(),
        });
        env.storage()
            .persistent()
            .set(&DataKey::SatisfactionTrend(artist.clone()), &trend);

        env.events()
            .publish((symbol_short!("sat_rec"),), (artist, rating));
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Earnings predictions
    // -----------------------------------------------------------------------

    /// Compute and store an earnings prediction for an artist.
    ///
    /// Prediction = total_earnings / months_active, where months_active is
    /// derived from the artist's first recorded ledger to the current ledger.
    ///
    /// `first_recorded_ledger` should be passed by the caller (the platform
    /// service knows when the artist was first indexed).
    ///
    /// Admin only.
    ///
    /// Closes #602.
    pub fn compute_earnings_prediction(
        env: Env,
        artist: Address,
        first_recorded_ledger: u32,
    ) -> Result<(), AnalyticsError> {
        require_admin(&env)?;

        let stats = load_stats(&env, &artist);
        let current_ledger = env.ledger().sequence();
        let elapsed = current_ledger.saturating_sub(first_recorded_ledger);
        let months_active = (elapsed / LEDGERS_PER_MONTH).max(1);

        let predicted_monthly = stats.total_earnings / months_active as i128;
        let prediction = EarningsPrediction {
            predicted_monthly_earnings: predicted_monthly,
            months_active,
            computed_ledger: current_ledger,
        };
        env.storage()
            .persistent()
            .set(&DataKey::EarningsPrediction(artist.clone()), &prediction);

        env.events().publish(
            (symbol_short!("earn_pred"),),
            (artist, predicted_monthly),
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    /// Return top-level stats for an artist.
    ///
    /// Closes #602.
    pub fn get_artist_stats(env: Env, artist: Address) -> ArtistStats {
        load_stats(&env, &artist)
    }

    /// Return earnings for a specific category.
    ///
    /// Closes #602.
    pub fn get_category_earnings(
        env: Env,
        artist: Address,
        category: String,
    ) -> CategoryEarnings {
        env.storage()
            .persistent()
            .get(&DataKey::CategoryEarnings(artist.clone(), category.clone()))
            .unwrap_or(CategoryEarnings {
                category,
                earnings: 0,
                project_count: 0,
            })
    }

    /// Return all category tags for which earnings have been recorded.
    ///
    /// Closes #602.
    pub fn get_artist_categories(env: Env, artist: Address) -> Vec<String> {
        env.storage()
            .persistent()
            .get(&DataKey::ArtistCategories(artist))
            .unwrap_or(Vec::new(&env))
    }

    /// Return computed completion rate stats.
    ///
    /// Closes #602.
    pub fn get_completion_stats(env: Env, artist: Address) -> CompletionStats {
        let stats = load_stats(&env, &artist);
        CompletionStats {
            started: stats.projects_started,
            completed: stats.projects_completed,
            cancelled: stats.projects_cancelled,
            completion_rate_bps: compute_completion_rate(
                stats.projects_started,
                stats.projects_completed,
            ),
        }
    }

    /// Return response time aggregate.
    ///
    /// Closes #602.
    pub fn get_response_time_stats(env: Env, artist: Address) -> ResponseTimeStats {
        let stats = load_stats(&env, &artist);
        let avg = if stats.response_time_samples == 0 {
            0
        } else {
            (stats.total_response_time_ledgers / stats.response_time_samples as u64) as u32
        };
        ResponseTimeStats {
            avg_response_time_ledgers: avg,
            sample_count: stats.response_time_samples,
        }
    }

    /// Return the satisfaction trend data points for an artist.
    ///
    /// Closes #602.
    pub fn get_satisfaction_trend(env: Env, artist: Address) -> Vec<SatisfactionDataPoint> {
        env.storage()
            .persistent()
            .get(&DataKey::SatisfactionTrend(artist))
            .unwrap_or(Vec::new(&env))
    }

    /// Return the latest earnings prediction for an artist.
    ///
    /// Returns `None` (as `Option`) if no prediction has been computed yet.
    ///
    /// Closes #602.
    pub fn get_earnings_prediction(
        env: Env,
        artist: Address,
    ) -> Result<EarningsPrediction, AnalyticsError> {
        env.storage()
            .persistent()
            .get(&DataKey::EarningsPrediction(artist))
            .ok_or(AnalyticsError::NotFound)
    }

    /// Return overall average client satisfaction (0 if no ratings).
    ///
    /// Closes #602.
    pub fn get_avg_satisfaction(env: Env, artist: Address) -> u32 {
        let stats = load_stats(&env, &artist);
        if stats.satisfaction_count == 0 {
            return 0;
        }
        (stats.total_satisfaction / stats.satisfaction_count as u64) as u32
    }
}
