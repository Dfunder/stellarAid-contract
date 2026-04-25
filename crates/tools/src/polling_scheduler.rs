use std::time::Duration;

/// Interval at which the Stellar Horizon payment endpoint is polled.
const POLL_INTERVAL: Duration = Duration::from_secs(10);

/// Runs a blocking poll loop that calls `task` on every tick.
pub fn run_polling_loop<F>(mut task: F)
where
    F: FnMut(),
{
    loop {
        task();
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
