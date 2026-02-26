//! Horizon API Client
//!
//! A robust client for interacting with Stellar Horizon API with:
//! - Error handling for network and API errors
//! - Rate limiting to respect Horizon limits
//! - Retry logic with exponential backoff
//! - Request logging and health checks

pub mod cache;
pub mod health;
pub mod client;

pub use client::{HorizonClient, HorizonClientConfig};
