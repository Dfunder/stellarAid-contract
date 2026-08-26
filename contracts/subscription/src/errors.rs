use soroban_sdk::{contracterror, symbol_short, Symbol};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SubscriptionError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    TierNotFound = 4,
    TierExists = 5,
    TierInactive = 6,
    InvalidPrice = 7,
    InvalidPeriod = 8,
    AlreadySubscribed = 9,
    NoSubscription = 10,
    InsufficientCredit = 11,
    RenewalNotDue = 12,
    GraceExpired = 13,
    StillActive = 14,
    NotRenewable = 15,
    InvalidAmount = 16,
}

impl core::fmt::Display for SubscriptionError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::AlreadyInitialized => write!(f, "already initialized"),
            Self::NotInitialized => write!(f, "not initialized"),
            Self::Unauthorized => write!(f, "unauthorized"),
            Self::TierNotFound => write!(f, "tier not found"),
            Self::TierExists => write!(f, "tier already exists"),
            Self::TierInactive => write!(f, "tier is no longer offered"),
            Self::InvalidPrice => write!(f, "price must be positive"),
            Self::InvalidPeriod => write!(f, "period must be positive"),
            Self::AlreadySubscribed => write!(f, "already subscribed"),
            Self::NoSubscription => write!(f, "no subscription for this account"),
            Self::InsufficientCredit => write!(f, "not enough credit to cover the charge"),
            Self::RenewalNotDue => write!(f, "current period has not ended yet"),
            Self::GraceExpired => write!(f, "grace period has already elapsed"),
            Self::StillActive => write!(f, "subscription is still within its paid period"),
            Self::NotRenewable => write!(f, "subscription is not set to auto-renew"),
            Self::InvalidAmount => write!(f, "amount must be positive"),
        }
    }
}

pub fn get_suggestion(error: SubscriptionError) -> Symbol {
    match error {
        SubscriptionError::AlreadyInitialized => symbol_short!("DUP"),
        SubscriptionError::NotInitialized => symbol_short!("NO_INIT"),
        SubscriptionError::Unauthorized => symbol_short!("AUTH"),
        SubscriptionError::TierNotFound => symbol_short!("NO_TIER"),
        SubscriptionError::TierExists => symbol_short!("TIER_DUP"),
        SubscriptionError::TierInactive => symbol_short!("TIER_OFF"),
        SubscriptionError::InvalidPrice => symbol_short!("BAD_PRICE"),
        SubscriptionError::InvalidPeriod => symbol_short!("BAD_PER"),
        SubscriptionError::AlreadySubscribed => symbol_short!("SUB_DUP"),
        SubscriptionError::NoSubscription => symbol_short!("NO_SUB"),
        SubscriptionError::InsufficientCredit => symbol_short!("NO_CRED"),
        SubscriptionError::RenewalNotDue => symbol_short!("NOT_DUE"),
        SubscriptionError::GraceExpired => symbol_short!("NO_GRACE"),
        SubscriptionError::StillActive => symbol_short!("ACTIVE"),
        SubscriptionError::NotRenewable => symbol_short!("NO_RENEW"),
        SubscriptionError::InvalidAmount => symbol_short!("BAD_AMT"),
    }
}
