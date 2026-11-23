//! Token interface for escrow.
//!
//! Uses the Soroban SDK's built-in token interface instead of importing a
//! prebuilt WASM artifact, so `cargo check -p escrow` works from a clean
//! checkout without requiring `target/wasm32-unknown-unknown/release/` outputs.

pub use soroban_sdk::token::{Client, Interface, StellarAssetClient, TokenClient};
