#![cfg(test)]

extern crate std;

use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, String};

use crate::{
    errors::VerificationError,
    types::{BadgeStatus, BadgeType},
    VerificationContract, VerificationContractClient,
};

fn make_env() -> Env {
    Env::default()
}

fn setup(env: &Env) -> (Address, VerificationContractClient) {
    let cid = env.register_contract(None, VerificationContract);
    let client = VerificationContractClient::new(env, &cid);
    let admin = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin).unwrap();
    (admin, client)
}

fn b(env: &Env, s: &str) -> Bytes {
    Bytes::from_slice(env, s.as_bytes())
}

fn s(env: &Env, t: &str) -> String {
    String::from_str(env, t)
}

// ---------------------------------------------------------------------------
// Init
// ---------------------------------------------------------------------------

#[test]
fn test_double_init_fails() {
    let env = make_env();
    let cid = env.register_contract(None, VerificationContract);
    let client = VerificationContractClient::new(&env, &cid);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin).unwrap();
    let err = client.initialize(&admin).unwrap_err();
    assert_eq!(err, VerificationError::AlreadyInitialized);
}

// ---------------------------------------------------------------------------
// Request
// ---------------------------------------------------------------------------

#[test]
fn test_request_badge_happy_path() {
    let env = make_env();
    let (_, client) = setup(&env);
    let artist = Address::generate(&env);
    env.mock_all_auths();

    client
        .request_badge(&b(&env, "badge1"), &artist, &BadgeType::PortfolioVerified)
        .unwrap();

    let badge = client.get_badge(&b(&env, "badge1")).unwrap();
    assert_eq!(badge.status, BadgeStatus::Pending);
    assert_eq!(badge.artist, artist);
}

#[test]
fn test_duplicate_badge_request_blocked() {
    let env = make_env();
    let (_, client) = setup(&env);
    let artist = Address::generate(&env);
    env.mock_all_auths();

    client
        .request_badge(&b(&env, "badge1"), &artist, &BadgeType::PortfolioVerified)
        .unwrap();

    let err = client
        .request_badge(&b(&env, "badge2"), &artist, &BadgeType::PortfolioVerified)
        .unwrap_err();
    assert_eq!(err, VerificationError::AlreadyRequested);
}

// ---------------------------------------------------------------------------
// Approve
// ---------------------------------------------------------------------------

#[test]
fn test_approve_badge() {
    let env = make_env();
    let (_, client) = setup(&env);
    let artist = Address::generate(&env);
    env.mock_all_auths();

    client
        .request_badge(&b(&env, "badge1"), &artist, &BadgeType::IdVerified)
        .unwrap();

    // Approve with far-future expiry
    client.approve_badge(&b(&env, "badge1"), &999_999u32).unwrap();

    let badge = client.get_badge(&b(&env, "badge1")).unwrap();
    assert_eq!(badge.status, BadgeStatus::Active);
    assert_eq!(badge.expiry_ledger, 999_999);
}

#[test]
fn test_approve_no_expiry() {
    let env = make_env();
    let (_, client) = setup(&env);
    let artist = Address::generate(&env);
    env.mock_all_auths();

    client
        .request_badge(&b(&env, "badge1"), &artist, &BadgeType::TopCreator)
        .unwrap();
    client.approve_badge(&b(&env, "badge1"), &0u32).unwrap();

    let badge = client.get_badge(&b(&env, "badge1")).unwrap();
    assert_eq!(badge.expiry_ledger, 0);
    assert_eq!(badge.status, BadgeStatus::Active);
}

// ---------------------------------------------------------------------------
// Reject
// ---------------------------------------------------------------------------

#[test]
fn test_reject_badge() {
    let env = make_env();
    let (_, client) = setup(&env);
    let artist = Address::generate(&env);
    env.mock_all_auths();

    client
        .request_badge(&b(&env, "badge1"), &artist, &BadgeType::BackgroundChecked)
        .unwrap();
    client
        .reject_badge(&b(&env, "badge1"), &s(&env, "incomplete documents"))
        .unwrap();

    let badge = client.get_badge(&b(&env, "badge1")).unwrap();
    assert_eq!(badge.status, BadgeStatus::Rejected);

    // After rejection artist can re-apply
    client
        .request_badge(&b(&env, "badge2"), &artist, &BadgeType::BackgroundChecked)
        .unwrap();
}

// ---------------------------------------------------------------------------
// Revoke
// ---------------------------------------------------------------------------

#[test]
fn test_revoke_badge() {
    let env = make_env();
    let (_, client) = setup(&env);
    let artist = Address::generate(&env);
    env.mock_all_auths();

    client
        .request_badge(&b(&env, "badge1"), &artist, &BadgeType::AgencyVerified)
        .unwrap();
    client.approve_badge(&b(&env, "badge1"), &0u32).unwrap();
    client
        .revoke_badge(&b(&env, "badge1"), &s(&env, "policy violation"))
        .unwrap();

    let badge = client.get_badge(&b(&env, "badge1")).unwrap();
    assert_eq!(badge.status, BadgeStatus::Revoked);
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

#[test]
fn test_badge_history_tracked() {
    let env = make_env();
    let (_, client) = setup(&env);
    let artist = Address::generate(&env);
    env.mock_all_auths();

    client
        .request_badge(&b(&env, "badge1"), &artist, &BadgeType::PortfolioVerified)
        .unwrap();
    client.approve_badge(&b(&env, "badge1"), &0u32).unwrap();
    client
        .revoke_badge(&b(&env, "badge1"), &s(&env, "violation"))
        .unwrap();

    let history = client.get_badge_history(&b(&env, "badge1")).unwrap();
    // approve → revoke = 2 history entries
    assert_eq!(history.len(), 2);
    assert_eq!(history.get(0).unwrap().from_status, BadgeStatus::Pending);
    assert_eq!(history.get(0).unwrap().to_status, BadgeStatus::Active);
    assert_eq!(history.get(1).unwrap().from_status, BadgeStatus::Active);
    assert_eq!(history.get(1).unwrap().to_status, BadgeStatus::Revoked);
}
