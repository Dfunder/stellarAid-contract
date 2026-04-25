use std::thread;
use std::time::Duration;

/// Calls `op` up to `max_attempts` times, doubling the delay after each failure.
///
/// Returns `Ok(T)` on the first success, or the last `Err` if all attempts fail.
pub fn with_retry<T, E, F>(max_attempts: u32, initial_delay: Duration, mut op: F) -> Result<T, E>
where
    F: FnMut() -> Result<T, E>,
{
    let mut delay = initial_delay;
    let mut last_err: Option<E> = None;

    for attempt in 0..max_attempts {
        match op() {
            Ok(val) => return Ok(val),
            Err(e) => {
                last_err = Some(e);
                if attempt + 1 < max_attempts {
                    thread::sleep(delay);
                    delay *= 2;
                }
            }
        }
    }

    Err(last_err.expect("max_attempts must be > 0"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn succeeds_on_first_try() {
        let result: Result<i32, &str> = with_retry(3, Duration::from_millis(1), || Ok(42));
        assert_eq!(result, Ok(42));
    }

    #[test]
    fn retries_and_succeeds_on_third_attempt() {
        let mut calls = 0u32;
        let result: Result<i32, &str> = with_retry(5, Duration::from_millis(1), || {
            calls += 1;
            if calls < 3 { Err("transient") } else { Ok(1) }
        });
        assert_eq!(result, Ok(1));
        assert_eq!(calls, 3);
    }

    #[test]
    fn returns_last_error_when_all_attempts_fail() {
        let result: Result<i32, &str> = with_retry(3, Duration::from_millis(1), || Err("fail"));
        assert_eq!(result, Err("fail"));
    }
}
