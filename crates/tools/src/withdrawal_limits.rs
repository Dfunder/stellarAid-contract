use anyhow::{anyhow, Result};

/// Issue #139 – Add Withdrawal Limits
/// Defines per-campaign and global withdrawal rules and enforces them.

/// Configurable withdrawal limits for a campaign.
#[derive(Debug, Clone)]
pub struct WithdrawalLimits {
    /// Maximum single withdrawal amount (in stroops).
    pub max_per_withdrawal: i128,
    /// Minimum single withdrawal amount (in stroops).
    pub min_per_withdrawal: i128,
    /// Maximum total withdrawn across all withdrawals for the campaign.
    pub max_total: Option<i128>,
}

impl Default for WithdrawalLimits {
    fn default() -> Self {
        Self {
            min_per_withdrawal: 100,          // 100 stroops minimum
            max_per_withdrawal: 10_000_000_000, // 1000 XLM maximum per withdrawal
            max_total: None,                  // no global cap by default
        }
    }
}

impl WithdrawalLimits {
    pub fn new(min: i128, max: i128, max_total: Option<i128>) -> Result<Self> {
        if min <= 0 {
            return Err(anyhow!("Minimum withdrawal must be positive"));
        }
        if max < min {
            return Err(anyhow!("Maximum must be >= minimum"));
        }
        Ok(Self { min_per_withdrawal: min, max_per_withdrawal: max, max_total })
    }

    /// Validates a proposed withdrawal amount against the limits.
    /// `already_withdrawn` is the cumulative amount already withdrawn for this campaign.
    pub fn validate(&self, amount: i128, already_withdrawn: i128) -> Result<()> {
        if amount < self.min_per_withdrawal {
            return Err(anyhow!(
                "Withdrawal amount {} is below the minimum of {}",
                amount, self.min_per_withdrawal
            ));
        }
        if amount > self.max_per_withdrawal {
            return Err(anyhow!(
                "Withdrawal amount {} exceeds the maximum of {}",
                amount, self.max_per_withdrawal
            ));
        }
        if let Some(cap) = self.max_total {
            if already_withdrawn + amount > cap {
                return Err(anyhow!(
                    "Total withdrawn {} would exceed the campaign cap of {}",
                    already_withdrawn + amount, cap
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_limits_accept_normal_amount() {
        let limits = WithdrawalLimits::default();
        assert!(limits.validate(1_000_000, 0).is_ok());
    }

    #[test]
    fn rejects_below_minimum() {
        let limits = WithdrawalLimits::default();
        assert!(limits.validate(10, 0).is_err());
    }

    #[test]
    fn rejects_above_maximum() {
        let limits = WithdrawalLimits::default();
        assert!(limits.validate(20_000_000_000, 0).is_err());
    }

    #[test]
    fn rejects_when_total_cap_exceeded() {
        let limits = WithdrawalLimits::new(100, 5_000_000, Some(8_000_000)).unwrap();
        // already withdrawn 6_000_000, trying to withdraw 3_000_000 → total 9_000_000 > 8_000_000
        assert!(limits.validate(3_000_000, 6_000_000).is_err());
    }

    #[test]
    fn accepts_when_within_total_cap() {
        let limits = WithdrawalLimits::new(100, 5_000_000, Some(8_000_000)).unwrap();
        assert!(limits.validate(2_000_000, 5_000_000).is_ok());
    }

    #[test]
    fn constructor_rejects_invalid_range() {
        assert!(WithdrawalLimits::new(1000, 500, None).is_err());
        assert!(WithdrawalLimits::new(0, 500, None).is_err());
    }
}
