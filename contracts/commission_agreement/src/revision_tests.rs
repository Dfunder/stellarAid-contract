//! Tests for the Commission Revision System — closes #600.
#![cfg(test)]

extern crate std;

use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, String};

use crate::{
    errors::AgreementError,
    types::RevisionStatus,
    CommissionAgreementContract, CommissionAgreementContractClient,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_env() -> Env {
    Env::default()
}

fn b(env: &Env, s: &str) -> Bytes {
    Bytes::from_slice(env, s.as_bytes())
}

fn s(env: &Env, t: &str) -> String {
    String::from_str(env, t)
}

/// Create an Active agreement and return (client, artist, client_id).
fn setup_active_agreement(
    env: &Env,
    client: &CommissionAgreementContractClient,
) -> (Address, Address, Bytes) {
    let cl = Address::generate(env);
    let ar = Address::generate(env);
    let cid = b(env, "commission_1");

    env.mock_all_auths();
    client
        .create_agreement(&cid, &cl, &ar, &s(env, "Logo Design"), &10_000i128, &9_999_999u32)
        .unwrap();
    client.accept_agreement(&cid).unwrap();
    (cl, ar, cid)
}

// ---------------------------------------------------------------------------
// set_revision_limit
// ---------------------------------------------------------------------------

#[test]
fn test_set_revision_limit() {
    let env = make_env();
    let cid_contract = env.register_contract(None, CommissionAgreementContract);
    let client = CommissionAgreementContractClient::new(&env, &cid_contract);
    let (cl, _ar, cid) = setup_active_agreement(&env, &client);

    client.set_revision_limit(&cid, &cl, &3u32).unwrap();

    let config = client.get_revision_config(&cid).unwrap();
    assert_eq!(config.max_revisions, 3);
    assert_eq!(config.used_revisions, 0);
}

// ---------------------------------------------------------------------------
// request_revision
// ---------------------------------------------------------------------------

#[test]
fn test_request_revision_happy_path() {
    let env = make_env();
    let cid_contract = env.register_contract(None, CommissionAgreementContract);
    let client = CommissionAgreementContractClient::new(&env, &cid_contract);
    let (cl, _ar, cid) = setup_active_agreement(&env, &client);

    env.mock_all_auths();
    client
        .request_revision(
            &b(&env, "rev1"),
            &cid,
            &cl,
            &s(&env, "Change colours to blue"),
            &500i128,
            &9_999_998u32,
        )
        .unwrap();

    let revisions = client.get_revisions(&cid).unwrap();
    assert_eq!(revisions.len(), 1);
    assert_eq!(revisions.get(0).unwrap().status, RevisionStatus::Pending);
}

#[test]
fn test_request_revision_limit_enforced() {
    let env = make_env();
    let cid_contract = env.register_contract(None, CommissionAgreementContract);
    let client = CommissionAgreementContractClient::new(&env, &cid_contract);
    let (cl, ar, cid) = setup_active_agreement(&env, &client);

    env.mock_all_auths();
    // Limit to 1
    client.set_revision_limit(&cid, &cl, &1u32).unwrap();

    client
        .request_revision(
            &b(&env, "rev1"),
            &cid,
            &cl,
            &s(&env, "first revision"),
            &0i128,
            &9_999_998u32,
        )
        .unwrap();

    // Second revision should fail — limit reached.
    let err = client
        .request_revision(
            &b(&env, "rev2"),
            &cid,
            &ar,
            &s(&env, "second revision"),
            &0i128,
            &9_999_997u32,
        )
        .unwrap_err();
    assert_eq!(err, AgreementError::RevisionLimitReached);
}

#[test]
fn test_duplicate_revision_id_blocked() {
    let env = make_env();
    let cid_contract = env.register_contract(None, CommissionAgreementContract);
    let client = CommissionAgreementContractClient::new(&env, &cid_contract);
    let (cl, _ar, cid) = setup_active_agreement(&env, &client);

    env.mock_all_auths();
    client
        .request_revision(
            &b(&env, "rev1"),
            &cid,
            &cl,
            &s(&env, "change 1"),
            &0i128,
            &9_999_998u32,
        )
        .unwrap();

    let err = client
        .request_revision(
            &b(&env, "rev1"),
            &cid,
            &cl,
            &s(&env, "duplicate"),
            &0i128,
            &9_999_997u32,
        )
        .unwrap_err();
    assert_eq!(err, AgreementError::RevisionAlreadyExists);
}

// ---------------------------------------------------------------------------
// accept_revision
// ---------------------------------------------------------------------------

#[test]
fn test_accept_revision_applies_cost_adjustment() {
    let env = make_env();
    let cid_contract = env.register_contract(None, CommissionAgreementContract);
    let client = CommissionAgreementContractClient::new(&env, &cid_contract);
    let (cl, ar, cid) = setup_active_agreement(&env, &client);

    env.mock_all_auths();
    // Artist requests +2_000 USDC adjustment
    client
        .request_revision(
            &b(&env, "rev1"),
            &cid,
            &ar,
            &s(&env, "extra feature"),
            &2_000i128,
            &9_999_998u32,
        )
        .unwrap();

    // Client accepts
    client.accept_revision(&cid, &b(&env, "rev1"), &cl).unwrap();

    let rev = client.get_revisions(&cid).unwrap().get(0).unwrap();
    assert_eq!(rev.status, RevisionStatus::Accepted);

    // Budget should have increased
    let agreement = client.get_agreement(&cid).unwrap();
    assert_eq!(agreement.budget_usdc, 12_000);
}

#[test]
fn test_accept_revision_proposer_cannot_accept_own() {
    let env = make_env();
    let cid_contract = env.register_contract(None, CommissionAgreementContract);
    let client = CommissionAgreementContractClient::new(&env, &cid_contract);
    let (cl, ar, cid) = setup_active_agreement(&env, &client);

    env.mock_all_auths();
    client
        .request_revision(
            &b(&env, "rev1"),
            &cid,
            &cl,
            &s(&env, "change"),
            &0i128,
            &9_999_998u32,
        )
        .unwrap();

    // Client cannot accept their own revision.
    let err = client.accept_revision(&cid, &b(&env, "rev1"), &cl).unwrap_err();
    assert_eq!(err, AgreementError::Unauthorized);
}

// ---------------------------------------------------------------------------
// reject_revision
// ---------------------------------------------------------------------------

#[test]
fn test_reject_revision() {
    let env = make_env();
    let cid_contract = env.register_contract(None, CommissionAgreementContract);
    let client = CommissionAgreementContractClient::new(&env, &cid_contract);
    let (cl, ar, cid) = setup_active_agreement(&env, &client);

    env.mock_all_auths();
    client
        .request_revision(
            &b(&env, "rev1"),
            &cid,
            &ar,
            &s(&env, "extra work"),
            &1_000i128,
            &9_999_998u32,
        )
        .unwrap();

    client.reject_revision(&cid, &b(&env, "rev1"), &cl).unwrap();

    let rev = client.get_revisions(&cid).unwrap().get(0).unwrap();
    assert_eq!(rev.status, RevisionStatus::Rejected);

    // Budget should be unchanged after rejection
    let agreement = client.get_agreement(&cid).unwrap();
    assert_eq!(agreement.budget_usdc, 10_000);
}

// ---------------------------------------------------------------------------
// expire_revision
// ---------------------------------------------------------------------------

#[test]
fn test_expire_revision_too_early_fails() {
    let env = make_env();
    let cid_contract = env.register_contract(None, CommissionAgreementContract);
    let client = CommissionAgreementContractClient::new(&env, &cid_contract);
    let (cl, _ar, cid) = setup_active_agreement(&env, &client);

    env.mock_all_auths();
    // deadline very far in future
    client
        .request_revision(
            &b(&env, "rev1"),
            &cid,
            &cl,
            &s(&env, "change"),
            &0i128,
            &9_999_998u32,
        )
        .unwrap();

    let err = client.expire_revision(&cid, &b(&env, "rev1")).unwrap_err();
    assert_eq!(err, AgreementError::RevisionDeadlinePast);
}

// ---------------------------------------------------------------------------
// get_revision_config
// ---------------------------------------------------------------------------

#[test]
fn test_get_revision_config_defaults() {
    let env = make_env();
    let cid_contract = env.register_contract(None, CommissionAgreementContract);
    let client = CommissionAgreementContractClient::new(&env, &cid_contract);
    let (_cl, _ar, cid) = setup_active_agreement(&env, &client);

    let config = client.get_revision_config(&cid).unwrap();
    assert_eq!(config.max_revisions, 0); // unlimited by default
    assert_eq!(config.used_revisions, 0);
}
