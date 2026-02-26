use serde::{Deserialize, Serialize};

use super::error::{FeeError, FeeResult};

/// Surge pricing detection thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurgePricingConfig {
    /// Normal base fee in stroops (100)
    pub normal_base_fee: i64,
    /// Surge warning threshold percentage (e.g., 150 = warn at 1.5x)
    pub warn_threshold_percent: f64,
    /// Critical threshold percentage (e.g., 300 = critical at 3x)
    pub critical_threshold_percent: f64,
    /// Number of observations to track for trend detection
    pub window_size: usize,
}

impl Default for SurgePricingConfig {
    fn default() -> Self {
        Self {
            normal_base_fee: 100,
            warn_threshold_percent: 150.0,
            critical_threshold_percent: 300.0,
            window_size: 10,
        }
    }
}

/// Surge pricing level
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SurgePricingLevel {
    /// Normal pricing (0-100%)
    Normal,
    /// Warning level (100-150%)
    Elevated,
    /// Significant increase (150-300%)
    High,
    /// Critical surge (>300%)
    Critical,
}

impl SurgePricingLevel {
    /// Get user-friendly display name
    pub fn name(&self) -> &str {
        match self {
            SurgePricingLevel::Normal => "Normal",
            SurgePricingLevel::Elevated => "Elevated",
            SurgePricingLevel::High => "High",
            SurgePricingLevel::Critical => "Critical",
        }
    }

    /// Get description for users
    pub fn description(&self) -> &str {
        match self {
            SurgePricingLevel::Normal => "Network fees are normal",
            SurgePricingLevel::Elevated => "Network is slightly congested",
            SurgePricingLevel::High => "Network is congested",
            SurgePricingLevel::Critical => "Network is congested - high fees",
        }
    }
}

/// Surge pricing analyzer
#[derive(Debug)]
pub struct SurgePricingAnalyzer {
    config: SurgePricingConfig,
    fee_history: Vec<i64>,
}

impl SurgePricingAnalyzer {
    /// Create new surge pricing analyzer
    pub fn new(config: SurgePricingConfig) -> Self {
        Self {
            config,
            fee_history: Vec::new(),
        }
    }

    /// Analyze current fee and detect surge pricing
    pub fn analyze(&mut self, current_base_fee: i64) -> FeeResult<SurgePricingAnalysis> {
        if current_base_fee < 0 {
            return Err(FeeError::InvalidFeeValue(
                "base fee cannot be negative".to_string(),
            ));
        }

        // Add to history
        self.fee_history.push(current_base_fee);
        if self.fee_history.len() > self.config.window_size {
            self.fee_history.remove(0);
        }

        let surge_percent = if self.config.normal_base_fee > 0 {
            (current_base_fee as f64 / self.config.normal_base_fee as f64) * 100.0
        } else {
            100.0
        };

        let surge_level = self.detect_level(surge_percent);
        let is_surge = surge_level != SurgePricingLevel::Normal;
        let trend = self.calculate_trend();

        Ok(SurgePricingAnalysis {
            surge_level,
            is_surge,
            surge_percent,
            current_fee: current_base_fee,
            normal_fee: self.config.normal_base_fee,
            trend,
            recommendation: self.get_recommendation(surge_level),
        })
    }

    /// Detect surge pricing level
    fn detect_level(&self, surge_percent: f64) -> SurgePricingLevel {
        if surge_percent >= self.config.critical_threshold_percent {
            SurgePricingLevel::Critical
        } else if surge_percent >= self.config.warn_threshold_percent {
            SurgePricingLevel::High
        } else if surge_percent > 100.0 {
            SurgePricingLevel::Elevated
        } else {
            SurgePricingLevel::Normal
        }
    }

    /// Calculate fee trend (increasing, stable, decreasing)
    fn calculate_trend(&self) -> FeeTrend {
        if self.fee_history.len() < 2 {
            return FeeTrend::Stable;
        }

        let recent = &self.fee_history[self.fee_history.len() / 2..];
        let older = &self.fee_history[..self.fee_history.len() / 2];

        let recent_avg = recent.iter().sum::<i64>() as f64 / recent.len() as f64;
        let older_avg = older.iter().sum::<i64>() as f64 / older.len() as f64;

        let percent_change = ((recent_avg - older_avg) / older_avg) * 100.0;

        if percent_change > 10.0 {
            FeeTrend::Increasing
        } else if percent_change < -10.0 {
            FeeTrend::Decreasing
        } else {
            FeeTrend::Stable
        }
    }

    /// Get recommendation based on surge level
    fn get_recommendation(&self, level: SurgePricingLevel) -> String {
        match level {
            SurgePricingLevel::Normal => "Fees are normal. Safe to proceed.".to_string(),
            SurgePricingLevel::Elevated => {
                "Network is slightly congested. Fees are slightly elevated.".to_string()
            }
            SurgePricingLevel::High => {
                "Network is congested. Consider waiting if not urgent.".to_string()
            }
            SurgePricingLevel::Critical => {
                "Network has critical congestion. Wait for fees to decrease if possible.".to_string()
            }
        }
    }

    /// Reset fee history
    pub fn reset_history(&mut self) {
        self.fee_history.clear();
    }

    /// Get average fee from history
    pub fn average_fee(&self) -> Option<f64> {
        if self.fee_history.is_empty() {
            None
        } else {
            let sum: i64 = self.fee_history.iter().sum();
            Some(sum as f64 / self.fee_history.len() as f64)
        }
    }

    /// Get max fee from history
    pub fn max_fee(&self) -> Option<i64> {
        self.fee_history.iter().max().copied()
    }

    /// Get min fee from history
    pub fn min_fee(&self) -> Option<i64> {
        self.fee_history.iter().min().copied()
    }
}

/// Surge pricing analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurgePricingAnalysis {
    /// Detected surge pricing level
    pub surge_level: SurgePricingLevel,
    /// Is surge pricing active
    pub is_surge: bool,
    /// Current surge percentage (100 = normal, 200 = 2x)
    pub surge_percent: f64,
    /// Current base fee in stroops
    pub current_fee: i64,
    /// Normal base fee in stroops
    pub normal_fee: i64,
    /// Fee trend direction
    pub trend: FeeTrend,
    /// User-friendly recommendation
    pub recommendation: String,
}

/// Fee trend direction
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum FeeTrend {
    /// Fees increasing rapidly
    Increasing,
    /// Fees stable
    Stable,
    /// Fees decreasing
    Decreasing,
}

impl FeeTrend {
    /// Get emoji representation
    pub fn emoji(&self) -> &str {
        match self {
            FeeTrend::Increasing => "📈",
            FeeTrend::Stable => "➡️",
            FeeTrend::Decreasing => "📉",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_surge_pricing_config_default() {
        let config = SurgePricingConfig::default();
        assert_eq!(config.normal_base_fee, 100);
        assert_eq!(config.warn_threshold_percent, 150.0);
    }

    #[test]
    fn test_surge_level_normal() {
        let config = SurgePricingConfig::default();
        let analyzer = SurgePricingAnalyzer::new(config);

        // 100 stroops = normal (100%)
        let analysis = analyzer.detect_level(100.0);
        assert_eq!(analysis, SurgePricingLevel::Normal);
    }

    #[test]
    fn test_surge_level_elevated() {
        let config = SurgePricingConfig::default();
        let analyzer = SurgePricingAnalyzer::new(config);

        // 120 stroops = elevated (120%)
        let analysis = analyzer.detect_level(120.0);
        assert_eq!(analysis, SurgePricingLevel::Elevated);
    }

    #[test]
    fn test_surge_level_high() {
        let config = SurgePricingConfig::default();
        let analyzer = SurgePricingAnalyzer::new(config);

        // 200 stroops = high (200%)
        let analysis = analyzer.detect_level(200.0);
        assert_eq!(analysis, SurgePricingLevel::High);
    }

    #[test]
    fn test_surge_level_critical() {
        let config = SurgePricingConfig::default();
        let analyzer = SurgePricingAnalyzer::new(config);

        // 500 stroops = critical (500%)
        let analysis = analyzer.detect_level(500.0);
        assert_eq!(analysis, SurgePricingLevel::Critical);
    }

    #[test]
    fn test_surge_level_names() {
        assert_eq!(SurgePricingLevel::Normal.name(), "Normal");
        assert_eq!(SurgePricingLevel::Elevated.name(), "Elevated");
        assert_eq!(SurgePricingLevel::High.name(), "High");
        assert_eq!(SurgePricingLevel::Critical.name(), "Critical");
    }

    #[test]
    fn test_analyzer_normal_fee() {
        let config = SurgePricingConfig::default();
        let mut analyzer = SurgePricingAnalyzer::new(config);

        let analysis = analyzer.analyze(100).unwrap();
        assert_eq!(analysis.surge_level, SurgePricingLevel::Normal);
        assert!(!analysis.is_surge);
        assert_eq!(analysis.surge_percent, 100.0);
    }

    #[test]
    fn test_analyzer_surge_fee() {
        let config = SurgePricingConfig::default();
        let mut analyzer = SurgePricingAnalyzer::new(config);

        let analysis = analyzer.analyze(250).unwrap();
        assert_eq!(analysis.surge_level, SurgePricingLevel::High);
        assert!(analysis.is_surge);
    }

    #[test]
    fn test_analyzer_invalid_fee() {
        let config = SurgePricingConfig::default();
        let mut analyzer = SurgePricingAnalyzer::new(config);

        let result = analyzer.analyze(-100);
        assert!(result.is_err());
    }

    #[test]
    fn test_average_fee() {
        let config = SurgePricingConfig::default();
        let mut analyzer = SurgePricingAnalyzer::new(config);

        analyzer.analyze(100).unwrap();
        analyzer.analyze(200).unwrap();
        analyzer.analyze(300).unwrap();

        let avg = analyzer.average_fee().unwrap();
        assert_eq!(avg, 200.0);
    }

    #[test]
    fn test_fee_trend() {
        let config = SurgePricingConfig::default();
        let mut analyzer = SurgePricingAnalyzer::new(config);

        // Add increasing fees
        for fee in 100..110 {
            analyzer.analyze(fee).unwrap();
        }

        let analysis = analyzer.analyze(150).unwrap();
        assert_eq!(analysis.trend, FeeTrend::Increasing);
    }

    #[test]
    fn test_trend_emoji() {
        assert_eq!(FeeTrend::Increasing.emoji(), "📈");
        assert_eq!(FeeTrend::Stable.emoji(), "➡️");
        assert_eq!(FeeTrend::Decreasing.emoji(), "📉");
    }
}
