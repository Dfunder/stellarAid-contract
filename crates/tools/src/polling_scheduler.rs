use std::time::Duration;

use crate::worker_logger::{LogLevel, WorkerLogger};

/// Interval at which the Stellar Horizon payment endpoint is polled.
const POLL_INTERVAL: Duration = Duration::from_secs(10);

/// Runs a blocking poll loop that calls `task` on every tick.
/// Issue #128 – logs health status and alerts on consecutive failures.
pub fn run_polling_loop<F>(mut task: F)
where
    F: FnMut() -> Result<(), String>,
{
    let mut logger = WorkerLogger::new();
    loop {
        match task() {
            Ok(()) => logger.log(LogLevel::Info, "Poll cycle completed successfully"),
            Err(e) => {
                logger.log(LogLevel::Error, format!("Poll cycle failed: {e}"));
                if !logger.is_healthy() {
                    logger.log(LogLevel::Warn, format!("Worker health: {:?}", logger.health_status()));
                }
            }
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_interval_is_nonzero() {
        assert!(POLL_INTERVAL.as_secs() > 0);
    }
}
