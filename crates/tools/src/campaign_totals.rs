use std::collections::HashMap;

/// In-memory store for per-campaign donation totals.
///
/// In production replace the inner map with a database connection.
#[derive(Default)]
pub struct CampaignTotals {
    totals: HashMap<u64, i128>,
}

impl CampaignTotals {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `amount` to the running total for `campaign_id` and returns the new total.
    pub fn increment(&mut self, campaign_id: u64, amount: i128) -> i128 {
        let entry = self.totals.entry(campaign_id).or_insert(0);
        *entry += amount;
        *entry
    }

    /// Returns the current total for `campaign_id`, or 0 if none recorded yet.
    pub fn get(&self, campaign_id: u64) -> i128 {
        *self.totals.get(&campaign_id).unwrap_or(&0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_at_zero() {
        let totals = CampaignTotals::new();
        assert_eq!(totals.get(1), 0);
    }

    #[test]
    fn increments_correctly() {
        let mut totals = CampaignTotals::new();
        totals.increment(1, 500);
        totals.increment(1, 300);
        assert_eq!(totals.get(1), 800);
    }

    #[test]
    fn different_campaigns_are_independent() {
        let mut totals = CampaignTotals::new();
        totals.increment(1, 100);
        totals.increment(2, 200);
        assert_eq!(totals.get(1), 100);
        assert_eq!(totals.get(2), 200);
    }
}
