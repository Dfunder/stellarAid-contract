//! Portfolio Analytics Contract
//!
//! Tracks artist performance metrics including:
//! - Earnings by category and client (issue #602)
//! - Project completion rate
//! - Response time analytics
//! - Client satisfaction trends
//! - Earnings predictions (rolling average)
//!
//! Closes #602 – Implement Portfolio Analytics Contract.

#![no_std]

mod errors;
mod types;

#[cfg(test)]
mod tests;

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Bytes, Env, String};

use errors::AnalyticsError;
use types::{ArtistMetrics, DataKey, EarningsRecord};

/// Ledger TTL for persistent analytics data (~90 days at 6 s/ledger).
const ANALYTICS_TTL_LEDGERS: u32 = 1_296_000;

#[contract]
pub struct AnalyticsContract;

#[contractimpl]
impl AnalyticsContract {
    // ── Admin bootstrap ────────────────────────────────────────────────────

    /// Initialize the contract and set the admin address.
    ///
    /// May only be called once.
    pub fn initialize(env: Env, admin: Address) -> Result<(), AnalyticsError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(AnalyticsError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.events().publish((symbol_short!("init"),), (admin,));
        Ok(())
    }

    // ── Earnings recording ─────────────────────────────────────────────────

    /// Record a commission payout for an artist.
    ///
    /// Increments the artist's `total_earnings` and `completed_count`, appends
    /// a detailed [`EarningsRecord`] for category/client drill-down, and
    /// refreshes the aggregate TTL.
    ///
    /// Only callable by the admin (platform oracle).
    ///
    /// Closes #602 – track earnings by category/client.
    pub fn record_earning(
        env: Env,
        artist: Address,
        commission_id: Bytes,
        category: String,
        client: Address,
        amount: i128,
    ) -> Result<(), AnalyticsError> {
        let admin = Self::require_admin(&env)?;
        admin.require_auth();

        if amount <= 0 {
            return Err(AnalyticsError::InvalidAmount);
        }

        let mut metrics = Self::load_or_default_metrics(&env, &artist);

        // Update aggregate totals
        metrics.total_earnings = metrics
            .total_earnings
            .checked_add(amount)
            .ok_or(AnalyticsError::ArithmeticOverflow)?;
        metrics.completed_count = metrics
            .completed_count
            .checked_add(1)
            .ok_or(AnalyticsError::ArithmeticOverflow)?;
        metrics.last_updated_ledger = env.ledger().sequence();

        // Persist updated metrics
        Self::save_metrics(&env, &artist, &metrics);

        // Append detailed earning record
        let idx: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::EarningCount(artist.clone()))
            .unwrap_or(0u32);

        let record = EarningsRecord {
            artist: artist.clone(),
            commission_id: commission_id.clone(),
            category: category.clone(),
            client: client.clone(),
            amount,
            ledger: env.ledger().sequence(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::Earning(artist.clone(), idx), &record);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Earning(artist.clone(), idx), ANALYTICS_TTL_LEDGERS, ANALYTICS_TTL_LEDGERS);
        env.storage()
            .persistent()
            .set(&DataKey::EarningCount(artist.clone()), &(idx + 1));
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::EarningCount(artist.clone()), ANALYTICS_TTL_LEDGERS, ANALYTICS_TTL_LEDGERS);

        env.events().publish(
            (symbol_short!("earning"),),
            (artist, commission_id, category, amount),
        );
        Ok(())
    }

    /// Record a cancelled or refunded commission for an artist.
    ///
    /// Increments `cancelled_count` which is used for completion rate.
    ///
    /// Closes #602 – project completion rate tracking.
    pub fn record_cancellation(env: Env, artist: Address) -> Result<(), AnalyticsError> {
        let admin = Self::require_admin(&env)?;
        admin.require_auth();

        let mut metrics = Self::load_or_default_metrics(&env, &artist);
        metrics.cancelled_count = metrics
            .cancelled_count
            .checked_add(1)
            .ok_or(AnalyticsError::ArithmeticOverflow)?;
        metrics.last_updated_ledger = env.ledger().sequence();
        Self::save_metrics(&env, &artist, &metrics);

        env.events().publish((symbol_short!("cancel"),), (artist,));
        Ok(())
    }

    // ── Response time ──────────────────────────────────────────────────────

    /// Record how many ledgers passed between a commission request and the
    /// artist's first response.
    ///
    /// Adds to the rolling sum so callers can compute the average via
    /// [`get_avg_response_time`].
    ///
    /// Closes #602 – response time analytics.
    pub fn record_response_time(
        env: Env,
        artist: Address,
        response_ledgers: u64,
    ) -> Result<(), AnalyticsError> {
        let admin = Self::require_admin(&env)?;
        admin.require_auth();

        if response_ledgers == 0 {
            return Err(AnalyticsError::InvalidAmount);
        }

        let mut metrics = Self::load_or_default_metrics(&env, &artist);
        metrics.response_time_sum = metrics
            .response_time_sum
            .checked_add(response_ledgers)
            .ok_or(AnalyticsError::ArithmeticOverflow)?;
        metrics.response_time_count = metrics
            .response_time_count
            .checked_add(1)
            .ok_or(AnalyticsError::ArithmeticOverflow)?;
        metrics.last_updated_ledger = env.ledger().sequence();
        Self::save_metrics(&env, &artist, &metrics);

        env.events().publish(
            (symbol_short!("resp_time"),),
            (artist, response_ledgers),
        );
        Ok(())
    }

    // ── Client satisfaction ────────────────────────────────────────────────

    /// Record a client satisfaction score for a completed commission.
    ///
    /// `score_x10` is the score multiplied by 10 to preserve one decimal place
    /// (e.g. 4.5 stars → `45`). Valid range: 10–50.
    ///
    /// Closes #602 – client satisfaction trends.
    pub fn record_satisfaction(
        env: Env,
        artist: Address,
        score_x10: u32,
    ) -> Result<(), AnalyticsError> {
        let admin = Self::require_admin(&env)?;
        admin.require_auth();

        if score_x10 < 10 || score_x10 > 50 {
            return Err(AnalyticsError::InvalidScore);
        }

        let mut metrics = Self::load_or_default_metrics(&env, &artist);
        metrics.satisfaction_score_sum = metrics
            .satisfaction_score_sum
            .checked_add(score_x10)
            .ok_or(AnalyticsError::ArithmeticOverflow)?;
        metrics.satisfaction_score_count = metrics
            .satisfaction_score_count
            .checked_add(1)
            .ok_or(AnalyticsError::ArithmeticOverflow)?;
        metrics.last_updated_ledger = env.ledger().sequence();
        Self::save_metrics(&env, &artist, &metrics);

        env.events().publish(
            (symbol_short!("satisf"),),
            (artist, score_x10),
        );
        Ok(())
    }

    // ── Read-only queries ──────────────────────────────────────────────────

    /// Return the full [`ArtistMetrics`] aggregate for an artist.
    ///
    /// Closes #602 – read-back for all metric categories.
    pub fn get_metrics(env: Env, artist: Address) -> Result<ArtistMetrics, AnalyticsError> {
        env.storage()
            .persistent()
            .get(&DataKey::Metrics(artist))
            .ok_or(AnalyticsError::NotFound)
    }

    /// Return the completion rate as a percentage (0–100).
    ///
    /// `completion_rate = completed / (completed + cancelled) * 100`
    ///
    /// Returns 0 if no commissions have been recorded.
    ///
    /// Closes #602 – project completion rate tracking.
    pub fn get_completion_rate(env: Env, artist: Address) -> Result<u32, AnalyticsError> {
        let m = env
            .storage()
            .persistent()
            .get::<DataKey, ArtistMetrics>(&DataKey::Metrics(artist))
            .ok_or(AnalyticsError::NotFound)?;

        let total = m.completed_count + m.cancelled_count;
        if total == 0 {
            return Ok(0);
        }
        Ok((m.completed_count * 100) / total)
    }

    /// Return the average response time in ledgers (rounded down).
    ///
    /// Returns 0 if no response times have been recorded.
    ///
    /// Closes #602 – response time analytics.
    pub fn get_avg_response_time(env: Env, artist: Address) -> Result<u64, AnalyticsError> {
        let m = env
            .storage()
            .persistent()
            .get::<DataKey, ArtistMetrics>(&DataKey::Metrics(artist))
            .ok_or(AnalyticsError::NotFound)?;

        if m.response_time_count == 0 {
            return Ok(0);
        }
        Ok(m.response_time_sum / m.response_time_count as u64)
    }

    /// Return the average satisfaction score × 10 (rounded down).
    ///
    /// Returns 0 if no scores have been recorded.
    ///
    /// Closes #602 – client satisfaction trends.
    pub fn get_avg_satisfaction(env: Env, artist: Address) -> Result<u32, AnalyticsError> {
        let m = env
            .storage()
            .persistent()
            .get::<DataKey, ArtistMetrics>(&DataKey::Metrics(artist))
            .ok_or(AnalyticsError::NotFound)?;

        if m.satisfaction_score_count == 0 {
            return Ok(0);
        }
        Ok(m.satisfaction_score_sum / m.satisfaction_score_count)
    }

    /// Predict future earnings based on a simple rolling average of the last
    /// `completed_count` payouts.
    ///
    /// Returns `total_earnings / completed_count` (the mean payout per
    /// commission). Callers can project by multiplying this by an expected
    /// number of upcoming commissions.
    ///
    /// Returns 0 if no earnings have been recorded.
    ///
    /// Closes #602 – earnings predictions.
    pub fn predict_earnings(env: Env, artist: Address) -> Result<i128, AnalyticsError> {
        let m = env
            .storage()
            .persistent()
            .get::<DataKey, ArtistMetrics>(&DataKey::Metrics(artist))
            .ok_or(AnalyticsError::NotFound)?;

        if m.completed_count == 0 {
            return Ok(0);
        }
        Ok(m.total_earnings / m.completed_count as i128)
    }

    /// Return a single earnings record by its sequential index.
    ///
    /// Useful for paginated history display. Index starts at 0.
    pub fn get_earning(env: Env, artist: Address, index: u32) -> Result<EarningsRecord, AnalyticsError> {
        env.storage()
            .persistent()
            .get(&DataKey::Earning(artist, index))
            .ok_or(AnalyticsError::NotFound)
    }

    /// Return the total number of earnings records for an artist.
    pub fn get_earning_count(env: Env, artist: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::EarningCount(artist))
            .unwrap_or(0u32)
    }

    // ── Internal helpers ───────────────────────────────────────────────────

    fn require_admin(env: &Env) -> Result<Address, AnalyticsError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(AnalyticsError::NotInitialized)
    }

    fn load_or_default_metrics(env: &Env, artist: &Address) -> ArtistMetrics {
        env.storage()
            .persistent()
            .get(&DataKey::Metrics(artist.clone()))
            .unwrap_or(ArtistMetrics {
                total_earnings: 0,
                completed_count: 0,
                cancelled_count: 0,
                response_time_sum: 0,
                response_time_count: 0,
                satisfaction_score_sum: 0,
                satisfaction_score_count: 0,
                last_updated_ledger: 0,
            })
    }

    fn save_metrics(env: &Env, artist: &Address, metrics: &ArtistMetrics) {
        env.storage()
            .persistent()
            .set(&DataKey::Metrics(artist.clone()), metrics);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Metrics(artist.clone()), ANALYTICS_TTL_LEDGERS, ANALYTICS_TTL_LEDGERS);
    }
}
