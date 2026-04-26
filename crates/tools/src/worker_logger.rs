use std::fmt;

/// Severity level for a worker log entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Info  => write!(f, "INFO"),
            LogLevel::Warn  => write!(f, "WARN"),
            LogLevel::Error => write!(f, "ERROR"),
        }
    }
}

/// A single structured log entry produced by the worker.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub level:   LogLevel,
    pub message: String,
}

/// Issue #128 – health status of the worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Failed,
}

/// Minimal logger that stores entries in memory and prints them to stderr.
#[derive(Default)]
pub struct WorkerLogger {
    entries: Vec<LogEntry>,
    /// Minimum level that will be recorded and printed.
    pub min_level: Option<LogLevel>,
    /// Issue #128 – consecutive error count for health tracking.
    consecutive_errors: u32,
}

impl WorkerLogger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records and prints `message` at the given `level`.
    pub fn log(&mut self, level: LogLevel, message: impl Into<String>) {
        if self.min_level.map_or(true, |min| level >= min) {
            let entry = LogEntry { level, message: message.into() };
            eprintln!("[{}] {}", entry.level, entry.message);
            // Issue #128 – track consecutive errors for health monitoring
            if level == LogLevel::Error {
                self.consecutive_errors += 1;
                if self.consecutive_errors >= 3 {
                    eprintln!("[ALERT] Worker health degraded: {} consecutive errors", self.consecutive_errors);
                }
            } else {
                self.consecutive_errors = 0;
            }
            self.entries.push(entry);
        }
    }

    /// Returns all stored log entries.
    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }

    /// Issue #128 – return current health status based on consecutive errors.
    pub fn health_status(&self) -> HealthStatus {
        match self.consecutive_errors {
            0 => HealthStatus::Healthy,
            1..=2 => HealthStatus::Degraded,
            _ => HealthStatus::Failed,
        }
    }

    /// Issue #128 – returns true if the worker is healthy.
    pub fn is_healthy(&self) -> bool {
        self.health_status() == HealthStatus::Healthy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_starts_healthy() {
        let logger = WorkerLogger::new();
        assert_eq!(logger.health_status(), HealthStatus::Healthy);
        assert!(logger.is_healthy());
    }

    #[test]
    fn test_health_degrades_on_errors() {
        let mut logger = WorkerLogger::new();
        logger.log(LogLevel::Error, "err1");
        assert_eq!(logger.health_status(), HealthStatus::Degraded);
        logger.log(LogLevel::Error, "err2");
        assert_eq!(logger.health_status(), HealthStatus::Degraded);
        logger.log(LogLevel::Error, "err3");
        assert_eq!(logger.health_status(), HealthStatus::Failed);
    }

    #[test]
    fn test_health_resets_on_success() {
        let mut logger = WorkerLogger::new();
        logger.log(LogLevel::Error, "err");
        logger.log(LogLevel::Info, "ok");
        assert_eq!(logger.health_status(), HealthStatus::Healthy);
    }
}
