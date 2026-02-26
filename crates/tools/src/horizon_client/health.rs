//! Health Check for Horizon API
//!
//! Provides health check and status monitoring for Horizon.

use crate::horizon_error::{HorizonError, HorizonResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Health status of a service
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Service is healthy
    Healthy,
    /// Service is degraded but operational
    Degraded,
    /// Service is unhealthy
    Unhealthy,
    /// Unknown status (not checked yet)
    Unknown,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "Healthy"),
            HealthStatus::Degraded => write!(f, "Degraded"),
            HealthStatus::Unhealthy => write!(f, "Unhealthy"),
            HealthStatus::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    /// Service name
    pub service: String,
    /// Health status
    pub status: HealthStatus,
    /// Last check time
    pub last_check: DateTime<Utc>,
    /// Response time in milliseconds
    pub response_time_ms: u64,
    /// Error message if any
    pub error: Option<String>,
    /// Additional details
    pub details: Option<serde_json::Value>,
}

impl HealthCheckResult {
    /// Check if service is healthy
    pub fn is_healthy(&self) -> bool {
        matches!(self.status, HealthStatus::Healthy)
    }

    /// Check if service is operational (healthy or degraded)
    pub fn is_operational(&self) -> bool {
        matches!(
            self.status,
            HealthStatus::Healthy | HealthStatus::Degraded
        )
    }
}

/// Health check configuration
#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    /// Timeout for health checks
    pub timeout_ms: u64,
    /// How long to cache health check results
    pub cache_duration_ms: u64,
    /// Response time threshold for degraded status
    pub degraded_threshold_ms: u64,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 5000,
            cache_duration_ms: 30000,
            degraded_threshold_ms: 2000,
        }
    }
}

/// Horizon health checker
pub struct HorizonHealthChecker {
    /// Configuration
    config: HealthCheckConfig,
    /// Last health check result
    last_result: Arc<RwLock<Option<HealthCheckResult>>>,
}

impl HorizonHealthChecker {
    /// Create a new health checker
    pub fn new(config: HealthCheckConfig) -> Self {
        Self {
            config,
            last_result: Arc::new(RwLock::new(None)),
        }
    }

    /// Create with default configuration
    pub fn default_config() -> Self {
        Self::new(HealthCheckConfig::default())
    }

    /// Perform a health check on Horizon
    pub async fn check(&self, client: &crate::horizon_client::HorizonClient) -> HorizonResult<HealthCheckResult> {
        let start = std::time::Instant::now();

        // Get Horizon info
        match client.get("/").await {
            Ok(response) => {
                let response_time = start.elapsed().as_millis() as u64;

                let status = if response_time > self.config.degraded_threshold_ms {
                    HealthStatus::Degraded
                } else {
                    HealthStatus::Healthy
                };

                let result = HealthCheckResult {
                    service: "Horizon".to_string(),
                    status,
                    last_check: Utc::now(),
                    response_time_ms: response_time,
                    error: None,
                    details: Some(response),
                };

                *self.last_result.write().await = Some(result.clone());
                Ok(result)
            }
            Err(e) => {
                let response_time = start.elapsed().as_millis() as u64;

                let result = HealthCheckResult {
                    service: "Horizon".to_string(),
                    status: HealthStatus::Unhealthy,
                    last_check: Utc::now(),
                    response_time_ms: response_time,
                    error: Some(e.to_string()),
                    details: None,
                };

                *self.last_result.write().await = Some(result.clone());
                Err(e)
            }
        }
    }

    /// Get last health check result
    pub async fn last_result(&self) -> Option<HealthCheckResult> {
        self.last_result.read().await.clone()
    }

    /// Get last result from cache if available and fresh
    pub async fn last_result_if_fresh(&self) -> Option<HealthCheckResult> {
        if let Some(result) = self.last_result.read().await.clone() {
            let age_ms = (Utc::now() - result.last_check).num_milliseconds() as u64;
            if age_ms < self.config.cache_duration_ms {
                return Some(result);
            }
        }
        None
    }

    /// Clear cached result
    pub async fn clear_cache(&self) {
        *self.last_result.write().await = None;
    }

    /// Get configuration
    pub fn config(&self) -> &HealthCheckConfig {
        &self.config
    }
}

/// Continuous health monitoring
pub struct HealthMonitor {
    /// Health checker
    checker: HorizonHealthChecker,
    /// Check interval in seconds
    check_interval_secs: u64,
    /// Keep monitoring flag
    keep_monitoring: Arc<std::sync::atomic::AtomicBool>,
}

impl HealthMonitor {
    /// Create a new health monitor
    pub fn new(checker: HorizonHealthChecker, check_interval_secs: u64) -> Self {
        Self {
            checker,
            check_interval_secs,
            keep_monitoring: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Start continuous monitoring
    pub async fn start(&self, client: crate::horizon_client::HorizonClient) {
        self.keep_monitoring.store(true, std::sync::atomic::Ordering::Relaxed);

        let checker = self.checker.clone();
        let keep_monitoring = Arc::clone(&self.keep_monitoring);
        let interval = self.check_interval_secs;

        tokio::spawn(async move {
            while keep_monitoring.load(std::sync::atomic::Ordering::Relaxed) {
                match checker.check(&client).await {
                    Ok(result) => {
                        log::info!("Health check passed: {} ({}ms)", result.status, result.response_time_ms);
                    }
                    Err(e) => {
                        log::warn!("Health check failed: {}", e);
                    }
                }

                tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
            }
        });
    }

    /// Stop monitoring
    pub fn stop(&self) {
        self.keep_monitoring.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Clone for HorizonHealthChecker {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            last_result: Arc::clone(&self.last_result),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_display() {
        assert_eq!(HealthStatus::Healthy.to_string(), "Healthy");
        assert_eq!(HealthStatus::Degraded.to_string(), "Degraded");
        assert_eq!(HealthStatus::Unhealthy.to_string(), "Unhealthy");
    }

    #[test]
    fn test_health_check_result() {
        let result = HealthCheckResult {
            service: "Horizon".to_string(),
            status: HealthStatus::Healthy,
            last_check: Utc::now(),
            response_time_ms: 100,
            error: None,
            details: None,
        };

        assert!(result.is_healthy());
        assert!(result.is_operational());
    }

    #[test]
    fn test_health_check_degraded() {
        let result = HealthCheckResult {
            service: "Horizon".to_string(),
            status: HealthStatus::Degraded,
            last_check: Utc::now(),
            response_time_ms: 3000,
            error: None,
            details: None,
        };

        assert!(!result.is_healthy());
        assert!(result.is_operational());
    }

    #[test]
    fn test_health_check_unhealthy() {
        let result = HealthCheckResult {
            service: "Horizon".to_string(),
            status: HealthStatus::Unhealthy,
            last_check: Utc::now(),
            response_time_ms: 5000,
            error: Some("Connection refused".to_string()),
            details: None,
        };

        assert!(!result.is_healthy());
        assert!(!result.is_operational());
    }

    #[tokio::test]
    async fn test_health_checker_cache() {
        let checker = HorizonHealthChecker::default_config();

        // Should be None initially
        assert!(checker.last_result().await.is_none());
    }
}
