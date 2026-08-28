use soroban_sdk::{contracterror, symbol_short, Symbol};

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidFeeBps = 4,
    NoPendingAdmin = 5,
    AddressNotRegistered = 6,
}

impl core::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::AlreadyInitialized => write!(f, "contract already initialized"),
            Self::NotInitialized => write!(f, "contract not yet initialized"),
            Self::Unauthorized => write!(f, "caller is not authorized"),
            Self::InvalidFeeBps => write!(f, "fee basis points out of allowed range (0-10000)"),
            Self::NoPendingAdmin => write!(f, "no pending admin transfer requested"),
            Self::AddressNotRegistered => write!(f, "no address registered for that environment/name"),
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
        ConfigError::AddressNotRegistered => symbol_short!("NO_ADDR"),
    }
}
