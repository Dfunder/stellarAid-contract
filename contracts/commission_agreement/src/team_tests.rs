//! Team collaboration tests — closes #603.

#![cfg(test)]

extern crate std;

use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, String, Vec};

use crate::{
    errors::AgreementError,
    types::{PaymentSplitEntry, TeamRole},
    CommissionAgreementContract, CommissionAgreementContractClient,
};

fn make_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env
}

fn make_bytes(env: &Env, s: &str) -> Bytes {
    Bytes::from_slice(env, s.as_bytes())
}

fn make_str(env: &Env, s: &str) -> String {
    String::from_str(env, s)
}

fn setup_active_agreement(
    env: &Env,
) -> (CommissionAgreementContractClient, Address, Address, Bytes) {
    let contract_id = env.register_contract(None, CommissionAgreementContract);
    let client = CommissionAgreementContractClient::new(env, &contract_id);
    let commission_id = make_bytes(env, "comm-team-001");
    let payer = Address::generate(env);
    let artist = Address::generate(env);

    // Create + activate agreement.
    env.ledger().set_sequence_number(100);
    client
        .create_agreement(
            &commission_id,
            &payer,
            &artist,
            &make_str(env, "Team Project"),
            &50_000i128,
            &10_000u32,
        )
        .unwrap();
    client.accept_agreement(&commission_id).unwrap();

    (client, payer, artist, commission_id)
}

// ---------------------------------------------------------------------------
// add_team_member
// ---------------------------------------------------------------------------

#[test]
fn add_contributor_happy_path() {
    let env = make_env();
    let (client, _payer, artist, commission_id) = setup_active_agreement(&env);

    let contributor = Address::generate(&env);
    client
        .add_team_member(
            &commission_id,
            &artist,
            &contributor,
            &TeamRole::Contributor,
            &make_str(&env, "illustration"),
        )
        .unwrap();

    let members = client.get_team_members(&commission_id).unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members.get(0).unwrap().member, contributor);
    assert_eq!(members.get(0).unwrap().role, TeamRole::Contributor);
}

#[test]
fn add_viewer_happy_path() {
    let env = make_env();
    let (client, _payer, artist, commission_id) = setup_active_agreement(&env);

    let viewer = Address::generate(&env);
    client
        .add_team_member(
            &commission_id,
            &artist,
            &viewer,
            &TeamRole::Viewer,
            &make_str(&env, "review"),
        )
        .unwrap();

    let members = client.get_team_members(&commission_id).unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members.get(0).unwrap().role, TeamRole::Viewer);
}

#[test]
fn add_team_member_non_lead_rejected() {
    let env = make_env();
    let (client, payer, _artist, commission_id) = setup_active_agreement(&env);

    let contributor = Address::generate(&env);
    let err = client
        .add_team_member(
            &commission_id,
            &payer, // client, not the lead artist
            &contributor,
            &TeamRole::Contributor,
            &make_str(&env, "design"),
        )
        .unwrap_err();

    assert_eq!(err, AgreementError::TeamLeadRequired);
}

#[test]
fn add_team_member_duplicate_rejected() {
    let env = make_env();
    let (client, _payer, artist, commission_id) = setup_active_agreement(&env);

    let contributor = Address::generate(&env);
    client
        .add_team_member(
            &commission_id,
            &artist,
            &contributor,
            &TeamRole::Contributor,
            &make_str(&env, "art"),
        )
        .unwrap();

    let err = client
        .add_team_member(
            &commission_id,
            &artist,
            &contributor,
            &TeamRole::Viewer,
            &make_str(&env, "art"),
        )
        .unwrap_err();

    assert_eq!(err, AgreementError::TeamMemberAlreadyExists);
}

#[test]
fn add_lead_role_rejected() {
    let env = make_env();
    let (client, _payer, artist, commission_id) = setup_active_agreement(&env);

    let second_lead = Address::generate(&env);
    let err = client
        .add_team_member(
            &commission_id,
            &artist,
            &second_lead,
            &TeamRole::Lead, // not allowed
            &make_str(&env, "co-lead"),
        )
        .unwrap_err();

    assert_eq!(err, AgreementError::TeamLeadRequired);
}

// ---------------------------------------------------------------------------
// remove_team_member
// ---------------------------------------------------------------------------

#[test]
fn remove_team_member_happy_path() {
    let env = make_env();
    let (client, _payer, artist, commission_id) = setup_active_agreement(&env);

    let contributor = Address::generate(&env);
    client
        .add_team_member(
            &commission_id,
            &artist,
            &contributor,
            &TeamRole::Contributor,
            &make_str(&env, "animation"),
        )
        .unwrap();

    client
        .remove_team_member(&commission_id, &artist, &contributor)
        .unwrap();

    let members = client.get_team_members(&commission_id).unwrap();
    assert!(members.is_empty());
}

#[test]
fn remove_non_existent_member_fails() {
    let env = make_env();
    let (client, _payer, artist, commission_id) = setup_active_agreement(&env);

    let ghost = Address::generate(&env);
    let err = client
        .remove_team_member(&commission_id, &artist, &ghost)
        .unwrap_err();

    assert_eq!(err, AgreementError::TeamMemberNotFound);
}

// ---------------------------------------------------------------------------
// update_team_member_role
// ---------------------------------------------------------------------------

#[test]
fn update_role_contributor_to_viewer() {
    let env = make_env();
    let (client, _payer, artist, commission_id) = setup_active_agreement(&env);

    let member = Address::generate(&env);
    client
        .add_team_member(
            &commission_id,
            &artist,
            &member,
            &TeamRole::Contributor,
            &make_str(&env, "ui"),
        )
        .unwrap();

    client
        .update_team_member_role(&commission_id, &artist, &member, &TeamRole::Viewer)
        .unwrap();

    let members = client.get_team_members(&commission_id).unwrap();
    assert_eq!(members.get(0).unwrap().role, TeamRole::Viewer);
}

// ---------------------------------------------------------------------------
// set_payment_split
// ---------------------------------------------------------------------------

#[test]
fn payment_split_valid_two_way() {
    let env = make_env();
    let (client, _payer, artist, commission_id) = setup_active_agreement(&env);

    let contributor = Address::generate(&env);
    client
        .add_team_member(
            &commission_id,
            &artist,
            &contributor,
            &TeamRole::Contributor,
            &make_str(&env, "code"),
        )
        .unwrap();

    let entries = soroban_sdk::vec![
        &env,
        PaymentSplitEntry { member: artist.clone(), share_bps: 7000 },
        PaymentSplitEntry { member: contributor.clone(), share_bps: 3000 },
    ];

    client.set_payment_split(&commission_id, &artist, &entries).unwrap();

    let split = client.get_payment_split(&commission_id).unwrap();
    assert_eq!(split.len(), 2);
    let total: u32 = split.iter().map(|e| e.share_bps).sum();
    assert_eq!(total, 10_000);
}

#[test]
fn payment_split_wrong_bps_sum_rejected() {
    let env = make_env();
    let (client, _payer, artist, commission_id) = setup_active_agreement(&env);

    let contributor = Address::generate(&env);
    client
        .add_team_member(
            &commission_id,
            &artist,
            &contributor,
            &TeamRole::Contributor,
            &make_str(&env, "code"),
        )
        .unwrap();

    let entries = soroban_sdk::vec![
        &env,
        PaymentSplitEntry { member: artist.clone(), share_bps: 6000 },
        PaymentSplitEntry { member: contributor.clone(), share_bps: 3000 }, // sums to 9000
    ];

    let err = client
        .set_payment_split(&commission_id, &artist, &entries)
        .unwrap_err();
    assert_eq!(err, AgreementError::InvalidPaymentSplit);
}

#[test]
fn payment_split_unknown_member_rejected() {
    let env = make_env();
    let (client, _payer, artist, commission_id) = setup_active_agreement(&env);

    let stranger = Address::generate(&env);
    let entries = soroban_sdk::vec![
        &env,
        PaymentSplitEntry { member: artist.clone(), share_bps: 5000 },
        PaymentSplitEntry { member: stranger.clone(), share_bps: 5000 },
    ];

    let err = client
        .set_payment_split(&commission_id, &artist, &entries)
        .unwrap_err();
    assert_eq!(err, AgreementError::TeamMemberNotFound);
}

// ---------------------------------------------------------------------------
// Error code stability
// ---------------------------------------------------------------------------

#[test]
fn team_error_codes_are_stable() {
    assert_eq!(AgreementError::TeamMemberAlreadyExists as u32, 14);
    assert_eq!(AgreementError::TeamMemberNotFound as u32, 15);
    assert_eq!(AgreementError::TeamLeadRequired as u32, 16);
    assert_eq!(AgreementError::InvalidPaymentSplit as u32, 17);
    assert_eq!(AgreementError::MaxTeamSizeExceeded as u32, 18);
}
