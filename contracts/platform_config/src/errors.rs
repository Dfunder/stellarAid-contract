use soroban_sdk::{contracterror, symbol_short, Symbol};

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidFeeBps = 4,
    NoPendingAdmin = 5,
    InvalidTier = 6,
    InvalidPromotion = 7,
    InvalidReferralBps = 8,
    PromotionNotActive = 9,
}

impl core::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::AlreadyInitialized => write!(f, "contract already initialized"),
            Self::NotInitialized => write!(f, "contract not yet initialized"),
            Self::Unauthorized => write!(f, "caller is not authorized"),
            Self::InvalidFeeBps => write!(f, "fee basis points out of allowed range (0-10000)"),
            Self::NoPendingAdmin => write!(f, "no pending admin transfer requested"),
            Self::InvalidTier => write!(f, "fee tier has an invalid threshold or fee"),
            Self::InvalidPromotion => write!(f, "promotional period is invalid or out of range"),
            Self::InvalidReferralBps => write!(f, "referral share out of allowed range (0-10000)"),
            Self::PromotionNotActive => write!(f, "no promotion is active at the given ledger"),
        }
    }
}

pub fn get_suggestion(error: ConfigError) -> Symbol {
    match error {
        ConfigError::AlreadyInitialized => symbol_short!("DUP"),
        ConfigError::NotInitialized => symbol_short!("NO_INIT"),
        ConfigError::Unauthorized => symbol_short!("AUTH"),
        ConfigError::InvalidFeeBps => symbol_short!("BAD_BPS"),
        ConfigError::NoPendingAdmin => symbol_short!("NO_ADM"),
        ConfigError::InvalidTier => symbol_short!("BAD_TIER"),
        ConfigError::InvalidPromotion => symbol_short!("BAD_PRO"),
        ConfigError::InvalidReferralBps => symbol_short!("BAD_REF"),
        ConfigError::PromotionNotActive => symbol_short!("NO_PRO"),
    }
}
