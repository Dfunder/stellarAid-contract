use tokio::time::{sleep, Duration};
use tracing::warn;

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub backoff_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            base_delay_ms: 200,
            max_delay_ms: 10_000,
            backoff_factor: 2.0,
        }
    }
}

pub async fn retry_async<F, Fut, T, E>(config: &RetryConfig, f: F) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                if attempt >= config.max_attempts {
                    return Err(e);
                }
                let delay = calculate_delay(config, attempt);
                warn!(
                    attempt = attempt,
                    max_attempts = config.max_attempts,
                    delay_ms = delay,
                    "retryable operation failed, retrying"
                );
                sleep(Duration::from_millis(delay)).await;
            }
        }
    }
}

fn calculate_delay(config: &RetryConfig, attempt: u32) -> u64 {
    let delay = config.base_delay_ms as f64 * config.backoff_factor.powi(attempt as i32 - 1);
    (delay as u64).min(config.max_delay_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn retry_succeeds_on_first_attempt() {
        let config = RetryConfig {
            max_attempts: 3,
            ..Default::default()
        };
        let result = retry_async(&config, || async { Ok::<_, &str>(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn retry_succeeds_after_failures() {
        let config = RetryConfig {
            max_attempts: 5,
            base_delay_ms: 10,
            ..Default::default()
        };
        let counter = AtomicU32::new(0);
        let result = retry_async(&config, || async {
            let count = counter.fetch_add(1, Ordering::SeqCst);
            if count < 2 {
                Err::<&str, &str>("not yet")
            } else {
                Ok("finally")
            }
        })
        .await;
        assert_eq!(result.unwrap(), "finally");
    }

    #[tokio::test]
    async fn retry_exhausts_attempts() {
        let config = RetryConfig {
            max_attempts: 3,
            base_delay_ms: 10,
            ..Default::default()
        };
        let counter = AtomicU32::new(0);
        let result = retry_async(&config, || async {
            counter.fetch_add(1, Ordering::SeqCst);
            Err::<&str, &str>("always fails")
        })
        .await;
        assert_eq!(result.unwrap_err(), "always fails");
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn delay_calculation_backs_off() {
        let config = RetryConfig {
            base_delay_ms: 100,
            max_delay_ms: 10_000,
            backoff_factor: 2.0,
            ..Default::default()
        };
        assert_eq!(calculate_delay(&config, 1), 100);
        assert_eq!(calculate_delay(&config, 2), 200);
        assert_eq!(calculate_delay(&config, 3), 400);
        assert_eq!(calculate_delay(&config, 4), 800);
    }

    #[test]
    fn delay_capped_at_max() {
        let config = RetryConfig {
            base_delay_ms: 1000,
            max_delay_ms: 2500,
            backoff_factor: 2.0,
            ..Default::default()
        };
        assert_eq!(calculate_delay(&config, 1), 1000);
        assert_eq!(calculate_delay(&config, 2), 2000);
        assert_eq!(calculate_delay(&config, 3), 2500);
        assert_eq!(calculate_delay(&config, 4), 2500);
    }
}
