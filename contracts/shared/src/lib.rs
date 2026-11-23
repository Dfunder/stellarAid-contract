//! Shared types and helpers used across the StellarAid contracts.
//!
//! This crate is the dependency root of the workspace, so its public items are
//! the integration surface other crates compile against.

#![no_std]

// Closes #943: one layout for every lib.rs — `mod` declarations together at
// the top, then the `pub use` block. Ordering is alphabetical so a new module
// has an obvious place to go.
pub mod admin;
pub mod circuit_breaker;
pub mod config;
pub mod correlation;
pub mod dispute;
pub mod errors;
pub mod guard;
pub mod health;
pub mod initialization;
pub mod pagination;
pub mod pause;
pub mod rollout;
pub mod storage;
pub mod types;
pub mod upgrade;
pub mod validation;
pub mod version;

pub use admin::{
    AdminError, AdminKey, AdminRotatedEvent, AdminTransferCancelledEvent,
    AdminTransferProposedEvent,
};
pub use circuit_breaker::{
    CircuitBreakerConfig, CircuitBreakerError, CircuitBreakerState,
};
pub use health::{
    AlertConfig, HealthMetrics, HealthReport, HealthSnapshot, HealthStatus, SlaTargets,
};
pub use pagination::{
    clamp_limit, collect_window, page_info, paginate, paginated, window, PageInfo, DEFAULT_LIMIT,
    MAX_LIMIT,
};
pub use rollout::{
    flag_cap, set_flag_cap, RolloutError, RolloutPhase, RolloutState, MAX_FEATURE_FLAGS,
};
