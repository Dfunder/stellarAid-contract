//! Multi-contract integration testing framework (#663).
//!
//! In-process Soroban scenario harness that crosses contract boundaries
//! (platform config, escrow, commission, tokens). Lives under `tests/framework`
//! and runs as a member of the repo workspace via `cargo test -p
//! integration_framework`.
//!
//! Modules:
//! - [`environment`]: opinionated in-process test environment (tokens, auth).
//! - [`fixtures`]: deployed contract state ([`World`]).
//! - [`dsl`]: contract interaction steps.
//! - [`scenario`]: step-scripting orchestrator.
//! - [`assertions`]: state/event assertion helpers.

pub mod assertions;
pub mod commission;
pub mod config;
pub mod dsl;
pub mod environment;
pub mod fixtures;
pub mod scenario;

pub use environment::Environment;
pub use fixtures::{deploy_commission_stub, World};
pub use scenario::Scenario;