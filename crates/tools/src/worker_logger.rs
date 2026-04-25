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

/// Minimal logger that stores entries in memory and prints them to stderr.
#[derive(Default)]
pub struct WorkerLogger {
    entries: Vec<LogEntry>,
    /// Minimum level that will be recorded and printed.
    pub min_level: Option<LogLevel>,
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
            self.entries.push(entry);
        }
    }

    /// Returns all stored log entries.
    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }
}
