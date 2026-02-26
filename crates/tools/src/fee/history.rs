use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::error::{FeeError, FeeResult};

/// Historical fee record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeRecord {
    /// Base fee in stroops
    pub base_fee_stroops: i64,
    /// When this fee was observed
    pub timestamp: DateTime<Utc>,
    /// Source of the fee (e.g., "Horizon API")
    pub source: String,
}

impl FeeRecord {
    /// Create new fee record
    pub fn new(base_fee_stroops: i64, source: String) -> FeeResult<Self> {
        if base_fee_stroops < 0 {
            return Err(FeeError::InvalidFeeValue(
                "base_fee_stroops cannot be negative".to_string(),
            ));
        }

        Ok(Self {
            base_fee_stroops,
            timestamp: Utc::now(),
            source,
        })
    }

    /// Get age of record in seconds
    pub fn age_seconds(&self) -> i64 {
        (Utc::now() - self.timestamp).num_seconds()
    }
}

/// Fee history statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeStats {
    /// Minimum fee in stroops
    pub min_fee: i64,
    /// Maximum fee in stroops
    pub max_fee: i64,
    /// Average fee in stroops
    pub avg_fee: f64,
    /// Median fee in stroops
    pub median_fee: i64,
    /// Standard deviation
    pub std_dev: f64,
    /// Total records tracked
    pub total_records: usize,
}

impl FeeStats {
    /// Calculate statistics from fee records
    pub fn calculate(records: &[FeeRecord]) -> Option<Self> {
        if records.is_empty() {
            return None;
        }

        let fees: Vec<i64> = records.iter().map(|r| r.base_fee_stroops).collect();

        let min_fee = *fees.iter().min().unwrap();
        let max_fee = *fees.iter().max().unwrap();

        let sum: i64 = fees.iter().sum();
        let avg_fee = sum as f64 / fees.len() as f64;

        // Calculate median
        let mut sorted = fees.clone();
        sorted.sort_unstable();
        let median_fee = if sorted.len() % 2 == 0 {
            (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) as i64 / 2
        } else {
            sorted[sorted.len() / 2]
        };

        // Calculate standard deviation
        let variance = fees
            .iter()
            .map(|&f| {
                let diff = f as f64 - avg_fee;
                diff * diff
            })
            .sum::<f64>()
            / fees.len() as f64;
        let std_dev = variance.sqrt();

        Some(Self {
            min_fee,
            max_fee,
            avg_fee,
            median_fee,
            std_dev,
            total_records: fees.len(),
        })
    }
}

/// Fee history tracker
pub struct FeeHistory {
    records: Vec<FeeRecord>,
    max_records: usize,
}

impl FeeHistory {
    /// Create new fee history tracker
    pub fn new(max_records: usize) -> Self {
        Self {
            records: Vec::with_capacity(max_records),
            max_records,
        }
    }

    /// Create history with default capacity (1000 records)
    pub fn default_capacity() -> Self {
        Self::new(1000)
    }

    /// Add fee record
    pub fn add(&mut self, base_fee_stroops: i64, source: String) -> FeeResult<()> {
        let record = FeeRecord::new(base_fee_stroops, source)?;

        self.records.push(record);

        // Keep only most recent records
        if self.records.len() > self.max_records {
            self.records.remove(0);
        }

        Ok(())
    }

    /// Get all records
    pub fn all(&self) -> &[FeeRecord] {
        &self.records
    }

    /// Get records within time window (seconds)
    pub fn within_time_window(&self, seconds: i64) -> Vec<FeeRecord> {
        self.records
            .iter()
            .filter(|r| r.age_seconds() <= seconds)
            .cloned()
            .collect()
    }

    /// Get most recent record
    pub fn latest(&self) -> Option<&FeeRecord> {
        self.records.last()
    }

    /// Get oldest record
    pub fn oldest(&self) -> Option<&FeeRecord> {
        self.records.first()
    }

    /// Get record count
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Get statistics
    pub fn stats(&self) -> Option<FeeStats> {
        FeeStats::calculate(&self.records)
    }

    /// Get statistics for recent records
    pub fn recent_stats(&self, seconds: i64) -> Option<FeeStats> {
        let recent = self.within_time_window(seconds);
        FeeStats::calculate(&recent)
    }

    /// Clear history
    pub fn clear(&mut self) {
        self.records.clear();
    }

    /// Prune old records (older than seconds)
    pub fn prune_older_than(&mut self, seconds: i64) {
        self.records.retain(|r| r.age_seconds() <= seconds);
    }

    /// Get max fee change percentage over time window
    pub fn max_change_percent(&self, seconds: i64) -> Option<f64> {
        let recent = self.within_time_window(seconds);
        if recent.len() < 2 {
            return None;
        }

        let fees: Vec<i64> = recent.iter().map(|r| r.base_fee_stroops).collect();
        let min = *fees.iter().min()?;
        let max = *fees.iter().max()?;

        if min == 0 {
            return None;
        }

        let percent_change = ((max - min) as f64 / min as f64) * 100.0;
        Some(percent_change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fee_record_creation() {
        let record = FeeRecord::new(100, "Horizon".to_string()).unwrap();
        assert_eq!(record.base_fee_stroops, 100);
        assert_eq!(record.source, "Horizon");
    }

    #[test]
    fn test_fee_record_invalid() {
        let result = FeeRecord::new(-100, "Horizon".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_fee_history_add() {
        let mut history = FeeHistory::new(10);
        history.add(100, "Horizon".to_string()).unwrap();
        history.add(110, "Horizon".to_string()).unwrap();

        assert_eq!(history.len(), 2);
    }

    #[test]
    fn test_fee_history_latest() {
        let mut history = FeeHistory::new(10);
        history.add(100, "Horizon".to_string()).unwrap();
        history.add(150, "Horizon".to_string()).unwrap();

        assert_eq!(history.latest().unwrap().base_fee_stroops, 150);
    }

    #[test]
    fn test_fee_history_oldest() {
        let mut history = FeeHistory::new(10);
        history.add(100, "Horizon".to_string()).unwrap();
        history.add(150, "Horizon".to_string()).unwrap();

        assert_eq!(history.oldest().unwrap().base_fee_stroops, 100);
    }

    #[test]
    fn test_fee_history_capacity_limit() {
        let mut history = FeeHistory::new(3);
        history.add(100, "Horizon".to_string()).unwrap();
        history.add(110, "Horizon".to_string()).unwrap();
        history.add(120, "Horizon".to_string()).unwrap();
        history.add(130, "Horizon".to_string()).unwrap();

        assert_eq!(history.len(), 3);
        // Oldest should be removed
        assert_eq!(history.oldest().unwrap().base_fee_stroops, 110);
    }

    #[test]
    fn test_fee_stats_calculation() {
        let records = vec![
            FeeRecord::new(100, "Horizon".to_string()).unwrap(),
            FeeRecord::new(150, "Horizon".to_string()).unwrap(),
            FeeRecord::new(200, "Horizon".to_string()).unwrap(),
        ];

        let stats = FeeStats::calculate(&records).unwrap();
        assert_eq!(stats.min_fee, 100);
        assert_eq!(stats.max_fee, 200);
        assert_eq!(stats.avg_fee, 150.0);
        assert_eq!(stats.total_records, 3);
    }

    #[test]
    fn test_fee_stats_median() {
        let records = vec![
            FeeRecord::new(100, "Horizon".to_string()).unwrap(),
            FeeRecord::new(150, "Horizon".to_string()).unwrap(),
            FeeRecord::new(200, "Horizon".to_string()).unwrap(),
            FeeRecord::new(250, "Horizon".to_string()).unwrap(),
        ];

        let stats = FeeStats::calculate(&records).unwrap();
        assert_eq!(stats.median_fee, 175); // (150 + 200) / 2
    }

    #[test]
    fn test_fee_history_clear() {
        let mut history = FeeHistory::new(10);
        history.add(100, "Horizon".to_string()).unwrap();
        assert_eq!(history.len(), 1);

        history.clear();
        assert_eq!(history.len(), 0);
        assert!(history.is_empty());
    }

    #[test]
    fn test_fee_history_stats() {
        let mut history = FeeHistory::new(10);
        history.add(100, "Horizon".to_string()).unwrap();
        history.add(150, "Horizon".to_string()).unwrap();
        history.add(200, "Horizon".to_string()).unwrap();

        let stats = history.stats().unwrap();
        assert_eq!(stats.min_fee, 100);
        assert_eq!(stats.max_fee, 200);
        assert_eq!(stats.total_records, 3);
    }

    #[test]
    fn test_fee_history_within_time_window() {
        let mut history = FeeHistory::new(10);
        history.add(100, "Horizon".to_string()).unwrap();

        // Recent record should be within window
        let recent = history.within_time_window(60);
        assert_eq!(recent.len(), 1);
    }
}
