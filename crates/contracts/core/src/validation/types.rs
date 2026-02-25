//! Stellar Address Types
//!
//! Defines strongly-typed representations for Stellar addresses to ensure type safety
//! throughout the contract.

use soroban_sdk::{contracttype, String};

/// Represents a validated Stellar public key address
/// Standard format: 56 characters starting with 'G'
#[contracttype]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StellarAddress {
    pub address: String,
}

/// Represents a validated Stellar muxed account address
/// Muxed format: 69 characters starting with 'M'
#[contracttype]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MuxedAddress {
    pub address: String,
    pub id: u64,
}

/// Enum representing either a standard or muxed Stellar address
#[contracttype]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StellarAccount {
    Standard(StellarAddress),
    Muxed(MuxedAddress),
}

impl StellarAddress {
    /// Create a new StellarAddress from a validated string
    pub fn new(address: String) -> Self {
        Self { address }
    }
    
    /// Get the address as string
    pub fn as_str(&self) -> &String {
        &self.address
    }
}

impl MuxedAddress {
    /// Create a new MuxedAddress from a validated string and ID
    pub fn new(address: String, id: u64) -> Self {
        Self { address, id }
    }
    
    /// Get the address as string
    pub fn as_str(&self) -> &String {
        &self.address
    }
    
    /// Get the muxed ID
    pub fn id(&self) -> u64 {
        self.id
    }
}