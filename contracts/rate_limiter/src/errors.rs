//! Rate Limiter Errors
//!
//! Error definitions for the rate limiter module.

use soroban_sdk::{Diagnostic, Symbol, String, IntoVal, Env};

/// Custom error types for the rate limiter
#[derive(Debug, Clone)]
#[repr(u32)]
pub enum RateLimitErrorType {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    InvalidLimit = 3,
    InvalidWindow = 4,
    LimitExceeded = 5,
}

/// Rate limiter errors
#[derive(Debug)]
pub struct RateLimitError {
    pub error_type: RateLimitErrorType,
    pub message: String,
}

impl RateLimitError {
    pub fn not_initialized() -> Self {
        Self {
            error_type: RateLimitErrorType::NotInitialized,
            message: String::from_str(&Env::default(), "Rate limiter not initialized"),
        }
    }

    pub fn already_initialized() -> Self {
        Self {
            error_type: RateLimitErrorType::AlreadyInitialized,
            message: String::from_str(&Env::default(), "Rate limiter already initialized"),
        }
    }

    pub fn invalid_limit() -> Self {
        Self {
            error_type: RateLimitErrorType::InvalidLimit,
            message: String::from_str(&Env::default(), "Invalid rate limit value"),
        }
    }

    pub fn invalid_window() -> Self {
        Self {
            error_type: RateLimitErrorType::InvalidWindow,
            message: String::from_str(&Env::default(), "Invalid rate limit window"),
        }
    }

    pub fn limit_exceeded(key: types::RateLimitKey, limit: u32, window_ledgers: u32, current_count: u32) -> Self {
        Self {
            error_type: RateLimitErrorType::LimitExceeded,
            message: String::from_str(&Env::default(), &format!(
                "Rate limit exceeded for {:?}: {} actions in {} ledgers (limit: {})",
                key, current_count, window_ledgers, limit
            )),
        }
    }
}

// Convert the error to a Soroban error
impl soroban_sdk::TryFromVal<Env, RateLimitError> for soroban_sdk::Error {
    type Error = RateLimitError;

    fn try_from_val(_env: &Env, val: &RateLimitError) -> Result<Self, Self::Error> {
        Ok(soroban_sdk::Error::Contract(soroban_sdk::ContractError {
            code: val.error_type as u32,
            message: val.message.clone(),
        }))
    }
}

// Convert the error to a diagnostic
impl std::fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.error_type, self.message)
    }
}

// Convert the error to a diagnostic
impl Diagnostic for RateLimitError {
    fn description(&self) -> &str {
        &self.message
    }

    fn symbol(&self) -> Symbol {
        Symbol::new(&Env::default(), "rate_limit_error")
    }

    fn end_user_message(&self) -> Option<String> {
        Some(self.message.clone())
    }
}
