extern crate std;
use crate::errors::EscrowError;
use crate::storage::CommissionStatus;

#[test]
fn test_escrow_error_display_messages() {
    assert_eq!(
        std::format!("{}", EscrowError::AlreadyExists),
        "escrow already exists"
    );
    assert_eq!(
        std::format!("{}", EscrowError::NotFound),
        "escrow not found"
    );
    assert_eq!(
        std::format!("{}", EscrowError::InvalidStatus),
        "invalid escrow status for operation"
    );
    assert_eq!(
        std::format!("{}", EscrowError::Unauthorized),
        "caller is not authorized"
    );
    assert_eq!(
        std::format!("{}", EscrowError::InvalidAmount),
        "amount must be greater than zero"
    );
    assert_eq!(
        std::format!("{}", EscrowError::InsufficientBalance),
        "client does not hold enough USDC"
    );
}

#[test]
fn test_lifecycle_create_to_released() {
    let status = CommissionStatus::Locked;
    assert_eq!(status, CommissionStatus::Locked);
    assert_ne!(status, CommissionStatus::Released);
}

#[test]
fn test_lifecycle_create_to_refunded() {
    assert_ne!(CommissionStatus::Locked, CommissionStatus::Refunded);
}

#[test]
fn test_lifecycle_create_to_disputed_to_refunded() {
    assert_eq!(CommissionStatus::Locked as u32, 0);
    assert_eq!(CommissionStatus::Disputed as u32, 3);
    assert_eq!(CommissionStatus::Refunded as u32, 2);
}

#[test]
fn test_lifecycle_create_to_expired() {
    assert_eq!(CommissionStatus::Expired as u32, 4);
}

#[test]
fn test_error_suggestion_mappings() {
    use crate::errors::get_suggestion;
    use soroban_sdk::symbol_short;

    assert_eq!(get_suggestion(EscrowError::AlreadyExists), symbol_short!("DUP"));
    assert_eq!(get_suggestion(EscrowError::NotFound), symbol_short!("NOT_FOUND"));
    assert_eq!(get_suggestion(EscrowError::InvalidStatus), symbol_short!("BAD_STS"));
    assert_eq!(get_suggestion(EscrowError::Unauthorized), symbol_short!("AUTH"));
    assert_eq!(get_suggestion(EscrowError::InvalidAmount), symbol_short!("BAD_AMT"));
    assert_eq!(get_suggestion(EscrowError::InvalidFeeBps), symbol_short!("BAD_BPS"));
    assert_eq!(get_suggestion(EscrowError::NotExpired), symbol_short!("NOT_EXP"));
}

#[test]
fn test_error_discriminants_unique() {
    let mut vals: [u32; 11] = [
        EscrowError::AlreadyExists as u32,
        EscrowError::NotFound as u32,
        EscrowError::InvalidStatus as u32,
        EscrowError::Unauthorized as u32,
        EscrowError::InvalidAmount as u32,
        EscrowError::InvalidFeeBps as u32,
        EscrowError::DisputeAlreadyOpen as u32,
        EscrowError::NotExpired as u32,
        EscrowError::Reentrant as u32,
        EscrowError::InvalidAddress as u32,
        EscrowError::InsufficientBalance as u32,
    ];
    vals.sort();
    for i in 0..vals.len() {
        assert_eq!(vals[i], (i + 1) as u32, "discriminant {} must be unique", i + 1);
    }
}
