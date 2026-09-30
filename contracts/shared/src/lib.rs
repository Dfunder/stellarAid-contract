#![no_std]
pub mod config;
pub mod errors;
pub mod guard;
pub mod pause;
pub mod types;
pub mod upgrade;
pub mod health;
pub mod rollout;
pub mod correlation;
pub mod circuit_breaker;
pub mod pagination;

pub use health::{
    AlertConfig, HealthMetrics, HealthReport, HealthStatus, SlaTargets,
};
pub use pagination::{
    clamp_limit, collect_window, page_info, paginate, paginated, window, PageInfo, DEFAULT_LIMIT,
    MAX_LIMIT,
};
pub use rollout::{RolloutPhase, RolloutState};
pub use circuit_breaker::{
    CircuitBreakerConfig, CircuitBreakerError, CircuitBreakerState,
};
pub mod validation;
pub mod version;
