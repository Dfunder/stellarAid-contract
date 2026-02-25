//! Stellar Address Validation Utilities
//!
//! This module provides comprehensive validation for Stellar public keys and addresses,
//! including format validation, checksum verification, and support for both standard
//! and muxed accounts.

pub mod address;
pub mod errors;
pub mod types;

pub use address::*;
pub use errors::*;
pub use types::*;