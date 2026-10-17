use soroban_sdk::{contracterror, symbol_short, Symbol};

/// Typed errors returned by the NFT ownership certificate contract.
///
/// These replace the `assert!`/`expect` panics that previously guarded the
/// entry points, so a caller can handle each failure programmatically instead
/// of only observing a panic.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum NftError {
    /// The contract has not been initialized (no admin / royalty token).
    NotInitialized = 1,
    /// No certificate exists for the given id.
    NftNotFound = 2,
    /// The certificate is not active (for example, already burned).
    NotActive = 3,
    /// The caller is not the current owner of the certificate.
    NotOwner = 4,
    /// The certificate is marked non-transferable.
    NotTransferable = 5,
    /// The certificate is marked non-burnable.
    NotBurnable = 6,
    /// The caller is not the original creator of the certificate.
    NotCreator = 7,
    /// The caller is not the stored admin.
    Unauthorized = 8,
    /// The royalty basis points exceed the maximum of 3000 (30 %).
    RoyaltyTooHigh = 9,
    /// A negative sale price was supplied.
    NegativePrice = 10,
}

impl core::fmt::Display for NftError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::NotInitialized => write!(f, "contract not initialized"),
            Self::NftNotFound => write!(f, "NFT not found"),
            Self::NotActive => write!(f, "NFT is not active"),
            Self::NotOwner => write!(f, "caller is not the NFT owner"),
            Self::NotTransferable => write!(f, "this NFT cannot be transferred"),
            Self::NotBurnable => write!(f, "this NFT cannot be burned"),
            Self::NotCreator => write!(f, "only the creator can update royalties"),
            Self::Unauthorized => write!(f, "only admin can freeze"),
            Self::RoyaltyTooHigh => write!(f, "royalty_bps must not exceed 3000 (30%)"),
            Self::NegativePrice => write!(f, "price must be non-negative"),
        }
    }
}

#[allow(dead_code)]
pub fn get_suggestion(error: NftError) -> Symbol {
    match error {
        NftError::NotInitialized => symbol_short!("NO_INIT"),
        NftError::NftNotFound => symbol_short!("NOT_FOUND"),
        NftError::NotActive => symbol_short!("NOT_ACT"),
        NftError::NotOwner => symbol_short!("NOT_OWN"),
        NftError::NotTransferable => symbol_short!("NO_XFER"),
        NftError::NotBurnable => symbol_short!("NO_BURN"),
        NftError::NotCreator => symbol_short!("NOT_CRE"),
        NftError::Unauthorized => symbol_short!("AUTH"),
        NftError::RoyaltyTooHigh => symbol_short!("ROY_HIGH"),
        NftError::NegativePrice => symbol_short!("NEG_PRICE"),
    }
}
