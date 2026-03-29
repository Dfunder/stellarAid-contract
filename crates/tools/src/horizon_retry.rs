//! Retry Logic for Horizon API Requests
//!
//! Implements exponential backoff retry logic for transient failures.

use crate::horizon_error::{HorizonError, HorizonResult};
use std::time::Duration;

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Initial backoff duration
    pub initial_backoff: Duration,
    /// Maximum backoff duration
    pub max_backoff: Duration,
    /// Backoff multiplier (exponential)
    pub backoff_multiplier: f64,
    /// Whether to add jitter to backoff
    pub use_jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            use_jitter: true,
        }
    }
}

impl RetryConfig {
    /// Create a retry config for transient failures
    /// (3 attempts with exponential backoff)
    pub fn transient() -> Self {
        Self::default()
    }

    /// Create a conservative retry config
    /// (2 attempts with minimal backoff)
    pub fn conservative() -> Self {
        Self {
            max_attempts: 2,
            initial_backoff: Duration::from_millis(50),
            max_backoff: Duration::from_secs(5),
            backoff_multiplier: 2.0,
            use_jitter: true,
        }
    }

    /// Create an aggressive retry config
    /// (5 attempts with longer backoff)
    pub fn aggressive() -> Self {
        Self {
            max_attempts: 5,
            initial_backoff: Duration::from_millis(200),
            max_backoff: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            use_jitter: true,
        }
    }

    /// Create a no-retry config
    pub fn none() -> Self {
        Self {
            max_attempts: 1,
            initial_backoff: Duration::from_secs(0),
            max_backoff: Duration::from_secs(0),
            backoff_multiplier: 1.0,
            use_jitter: false,
        }
    }
}

/// Retry policy for handling failures
#[derive(Debug, Clone)]
pub enum RetryPolicy {
    /// Retry on transient errors only
    TransientOnly,
    /// Retry on transient errors and specific server errors
    TransientAndServerErrors,
    /// Retry on all retryable errors
    AllRetryable,
    /// Never retry
    NoRetry,
}

impl RetryPolicy {
    /// Check if an error should be retried
    pub fn should_retry(&self, error: &HorizonError) -> bool {
        match self {
            RetryPolicy::NoRetry => false,
            RetryPolicy::TransientOnly => {
                matches!(
                    error,
                    HorizonError::NetworkError(_)
                        | HorizonError::Timeout { .. }
                        | HorizonError::ConnectionRefused(_)
                        | HorizonError::ConnectionReset(_)
                        | HorizonError::DnsError(_)
                )
            },
            RetryPolicy::TransientAndServerErrors => {
                error.is_retryable() && (error.is_retryable() || error.is_server_error())
            },
            RetryPolicy::AllRetryable => error.is_retryable(),
        }
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy::TransientAndServerErrors
    }
}

/// Retry context for tracking retry attempts
pub struct RetryContext {
    /// Attempt number (1-indexed)
    pub attempt: u32,
    /// Total configured attempts
    pub max_attempts: u32,
    /// Delay before this attempt (if any)
    pub delay: Duration,
    /// Errors encountered so far
    pub errors: Vec<HorizonError>,
    /// Request ID for tracing
    pub request_id: Option<String>,
    /// Start time for tracking total duration
    pub started_at: std::time::Instant,
}

impl RetryContext {
    /// Create a new retry context
    pub fn new(max_attempts: u32) -> Self {
        Self {
            attempt: 1,
            max_attempts,
            delay: Duration::from_secs(0),
            errors: Vec::new(),
            request_id: None,
            started_at: std::time::Instant::now(),
        }
    }

    /// Set the request ID for tracing
    pub fn with_request_id(mut self, request_id: String) -> Self {
        self.request_id = Some(request_id);
        self
    }

    /// Check if we can retry
    pub fn can_retry(&self) -> bool {
        self.attempt < self.max_attempts
    }

    /// Get total attempts made
    pub fn attempts_made(&self) -> u32 {
        self.attempt
    }

    /// Get remaining attempts
    pub fn remaining_attempts(&self) -> u32 {
        self.max_attempts.saturating_sub(self.attempt)
    }

    /// Get total elapsed time since start
    pub fn elapsed_time(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Record an error and prepare for next attempt
    pub fn record_error(&mut self, error: HorizonError, next_delay: Duration) {
        self.errors.push(error);
        self.delay = next_delay;
        self.attempt += 1;
    }

    /// Get summary of retry attempts for logging
    pub fn get_retry_summary(&self) -> String {
        format!(
            "Total attempts: {}, Errors: {}, Elapsed: {:?}",
            self.attempt,
            self.errors.len(),
            self.elapsed_time()
        )
    }
}
    /// Get last error
    pub fn last_error(&self) -> Option<&HorizonError> {
        self.errors.last()
    }

    /// Check if this is the last attempt
    pub fn is_last_attempt(&self) -> bool {
        self.attempt == self.max_attempts
    }
}

/// Calculate backoff duration for a given attempt
pub fn calculate_backoff(attempt: u32, config: &RetryConfig) -> Duration {
    if attempt == 0 {
        return Duration::from_secs(0);
    }

    // Calculate exponential backoff: initial * (multiplier ^ (attempt - 1))
    let exp_backoff = config.initial_backoff.as_millis() as f64
        * config.backoff_multiplier.powi((attempt - 1) as i32);

    // Cap at max backoff
    let duration_ms = exp_backoff.min(config.max_backoff.as_millis() as f64) as u64;
    let mut backoff = Duration::from_millis(duration_ms);

    // Add jitter if enabled
    if config.use_jitter {
        // Add random jitter: ±10% of backoff
        let jitter_amount = (backoff.as_millis() as f64 * 0.1) as u64;
        let jitter = rand::random::<u64>() % (jitter_amount * 2);
        backoff = Duration::from_millis(
            (backoff.as_millis() as i64 - jitter_amount as i64 + jitter as i64).max(0) as u64,
        );
    }

    backoff
}

/// Retry a function with exponential backoff and enhanced logging
pub async fn retry_with_backoff<F, T>(
    config: &RetryConfig,
    policy: &RetryPolicy,
    mut f: F,
) -> HorizonResult<T>
where
    F: FnMut() -> futures::future::BoxFuture<'static, HorizonResult<T>>,
{
    let mut context = RetryContext::new(config.max_attempts);
    
    for attempt in 1..=config.max_attempts {
        context.attempt = attempt;
        
        match f().await {
            Ok(result) => {
                // Log success, especially if it required retries
                if attempt > 1 {
                    log::info!(
                        "Request succeeded after {} attempts (total time: {:?})",
                        attempt,
                        context.elapsed_time()
                    );
                }
                return Ok(result);
            },
            Err(error) => {
                // Get error details for logging
                let error_category = error.category();
                let error_severity = format!("{:?}", error.severity());
                let error_context = error.error_context();
                
                // Check if we should retry
                if !policy.should_retry(&error) {
                    log::warn!(
                        "[Attempt {}/{}] Non-retryable error ({}): {} - {}",
                        attempt,
                        config.max_attempts,
                        error_category,
                        error_severity,
                        error_context
                    );
                    return Err(error);
                }

                // Check if we have more attempts
                if attempt >= config.max_attempts {
                    log::error!(
                        "[Attempt {}/{}] Final attempt failed. Total time: {:?}. Error: {}",
                        attempt,
                        config.max_attempts,
                        context.elapsed_time(),
                        error_context
                    );
                    return Err(error);
                }

                // Calculate backoff
                let backoff = calculate_backoff(attempt, config);
                
                // Log retry attempt with detailed information
                log::warn!(
                    "[Attempt {}/{}] Retryable error ({}): {} | Backoff: {:?} | Elapsed: {:?}",
                    attempt,
                    config.max_attempts,
                    error_category,
                    error_context,
                    backoff,
                    context.elapsed_time()
                );

                // Record error for tracking
                context.record_error(error, backoff);

                tokio::time::sleep(backoff).await;
            },
        }
    }

    Err(HorizonError::Other(
        "Retry loop exhausted without returning".to_string(),
    ))
}

// Re-export for use in macros
use futures;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_defaults() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
    }

    #[test]
    fn test_retry_config_conservative() {
        let config = RetryConfig::conservative();
        assert_eq!(config.max_attempts, 2);
    }

    #[test]
    fn test_retry_config_aggressive() {
        let config = RetryConfig::aggressive();
        assert_eq!(config.max_attempts, 5);
    }

    #[test]
    fn test_retry_policy_defaults() {
        let policy = RetryPolicy::default();
        let error = HorizonError::NetworkError("test".to_string());
        assert!(policy.should_retry(&error));
    }

    #[test]
    fn test_retry_policy_no_retry() {
        let policy = RetryPolicy::NoRetry;
        let error = HorizonError::NetworkError("test".to_string());
        assert!(!policy.should_retry(&error));
    }

    #[test]
    fn test_calculate_backoff() {
        let config = RetryConfig::default();

        // First retry has no backoff
        let backoff1 = calculate_backoff(0, &config);
        assert_eq!(backoff1, Duration::from_secs(0));

        // Second retry has initial backoff (±10% jitter can dip slightly below 100ms)
        let backoff2 = calculate_backoff(1, &config);
        assert!(backoff2.as_millis() >= 80);

        // Backoff increases exponentially (compare mean trend; jitter can overlap once)
        let backoff3 = calculate_backoff(2, &config);
        assert!(backoff3.as_millis() >= backoff2.as_millis());
    }

    #[test]
    fn test_retry_context() {
        let mut ctx = RetryContext {
            attempt: 1,
            max_attempts: 3,
            delay: Duration::from_secs(0),
            errors: vec![],
        };

        assert!(ctx.can_retry());
        assert_eq!(ctx.attempts_made(), 1);
        assert_eq!(ctx.remaining_attempts(), 2);
        assert!(!ctx.is_last_attempt());

        ctx.attempt = 3;
        assert!(!ctx.can_retry());
        assert!(ctx.is_last_attempt());
    }

    #[test]
    fn test_backoff_cap() {
        let config = RetryConfig {
            max_attempts: 10,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(5),
            backoff_multiplier: 10.0, // Very aggressive
            use_jitter: false,
        };

        // High attempt number should cap at max_backoff
        let backoff = calculate_backoff(10, &config);
        assert!(backoff <= config.max_backoff);
    }
}
