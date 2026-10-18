extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env, String, Vec,
};

struct TestFixture<'a> {
    env: Env,
    client: MentorshipProgramClient<'a>,
    _admin: Address,
    mentor: Address,
    mentee: Address,
    token_id: Address,
    contract_id: Address,
}

fn setup<'a>() -> TestFixture<'a> {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_sequence_number(100);

    let admin = Address::generate(&env);
    let mentor = Address::generate(&env);
    let mentee = Address::generate(&env);

    let contract_id = env.register_contract(None, MentorshipProgram);
    let client = MentorshipProgramClient::new(&env, &contract_id);

    client.initialize(&admin);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin).address();

    TestFixture {
        env,
        client,
        _admin: admin,
        mentor,
        mentee,
        token_id,
        contract_id,
    }
}

fn default_milestones(env: &Env) -> Vec<(String, String)> {
    let mut ms = Vec::new(env);
    ms.push_back((
        String::from_str(env, "Milestone 1"),
        String::from_str(env, "Complete introductory architecture design"),
    ));
    ms.push_back((
        String::from_str(env, "Milestone 2"),
        String::from_str(env, "Implement and test smart contract modules"),
    ));
    ms
}

#[test]
fn test_propose_and_accept_engagement() {
    let f = setup();
    let ms = default_milestones(&f.env);
    let desc = String::from_str(&f.env, "Rust/Soroban Mentorship 2026");

    let eng_id = f.client.propose_engagement(&f.mentor, &f.mentee, &desc, &ms, &1_000, &f.token_id);
    assert_eq!(eng_id, 1);

    let eng = f.client.get_engagement(&eng_id);
    assert_eq!(eng.mentor, f.mentor);
    assert_eq!(eng.mentee, f.mentee);
    assert!(eng.status == EngagementStatus::Proposed);

    // Accept engagement
    f.client.accept_engagement(&f.mentee, &eng_id);
    let eng_after = f.client.get_engagement(&eng_id);
    assert!(eng_after.status == EngagementStatus::Active);

    // Milestones check
    let stored_ms = f.client.get_milestones(&eng_id);
    assert_eq!(stored_ms.len(), 2);
    assert!(stored_ms.get(0).unwrap().status == MilestoneStatus::Pending);
}

#[test]
fn test_cancel_engagement() {
    let f = setup();
    let ms = default_milestones(&f.env);
    let desc = String::from_str(&f.env, "Mentorship to cancel");

    let eng_id = f.client.propose_engagement(&f.mentor, &f.mentee, &desc, &ms, &0, &f.token_id);
    f.client.accept_engagement(&f.mentee, &eng_id);

    // Mentor cancels
    f.client.cancel_engagement(&f.mentor, &eng_id);
    let eng = f.client.get_engagement(&eng_id);
    assert!(eng.status == EngagementStatus::Cancelled);
}

#[test]
fn test_milestone_review_rejection() {
    let f = setup();
    let ms = default_milestones(&f.env);
    let desc = String::from_str(&f.env, "Mentorship rejection flow");

    let eng_id = f.client.propose_engagement(&f.mentor, &f.mentee, &desc, &ms, &1_000, &f.token_id);
    f.client.accept_engagement(&f.mentee, &eng_id);

    // Mentee submits milestone 0
    f.client.submit_milestone(&f.mentee, &eng_id, &0);
    let ms_list = f.client.get_milestones(&eng_id);
    assert!(ms_list.get(0).unwrap().status == MilestoneStatus::Submitted);

    // Mentor rejects milestone 0
    f.client.review_milestone(&f.mentor, &eng_id, &0, &false);
    let ms_list_rejected = f.client.get_milestones(&eng_id);
    assert!(ms_list_rejected.get(0).unwrap().status == MilestoneStatus::Rejected);

    // Engagement is still active, not completed
    let eng = f.client.get_engagement(&eng_id);
    assert!(eng.status == EngagementStatus::Active);
}

#[test]
fn test_milestone_review_cei_and_auto_complete_with_compensation() {
    let f = setup();
    let ms = default_milestones(&f.env);
    let desc = String::from_str(&f.env, "Mentorship completion flow");
    let compensation: i128 = 2_500;

    // Fund the mentorship contract address with tokens for compensation
    let token_admin = token::StellarAssetClient::new(&f.env, &f.token_id);
    token_admin.mint(&f.contract_id, &compensation);

    let token_client = token::Client::new(&f.env, &f.token_id);
    assert_eq!(token_client.balance(&f.contract_id), 2_500);
    assert_eq!(token_client.balance(&f.mentor), 0);

    let eng_id = f.client.propose_engagement(&f.mentor, &f.mentee, &desc, &ms, &compensation, &f.token_id);
    f.client.accept_engagement(&f.mentee, &eng_id);

    // Submit and approve milestone 0
    f.client.submit_milestone(&f.mentee, &eng_id, &0);
    f.client.review_milestone(&f.mentor, &eng_id, &0, &true);

    let ms0 = f.client.get_milestones(&eng_id).get(0).unwrap();
    assert!(ms0.status == MilestoneStatus::Approved);
    assert!(ms0.approved_at > 0);

    // Status still Active, no compensation transferred yet
    let eng_mid = f.client.get_engagement(&eng_id);
    assert!(eng_mid.status == EngagementStatus::Active);
    assert_eq!(token_client.balance(&f.mentor), 0);
    assert_eq!(token_client.balance(&f.contract_id), 2_500);

    // Submit and approve milestone 1 (the final milestone)
    f.client.submit_milestone(&f.mentee, &eng_id, &1);
    f.client.review_milestone(&f.mentor, &eng_id, &1, &true);

    // CEI Verification:
    // 1. Engagement status is updated to Completed
    let eng_done = f.client.get_engagement(&eng_id);
    assert!(eng_done.status == EngagementStatus::Completed);
    assert!(eng_done.completed_at > 0);

    // 2. Both milestones are approved
    let all_ms = f.client.get_milestones(&eng_id);
    assert!(all_ms.iter().all(|m| m.status == MilestoneStatus::Approved));

    // 3. Compensation token transfer succeeded
    assert_eq!(token_client.balance(&f.mentor), 2_500);
    assert_eq!(token_client.balance(&f.contract_id), 0);

    // 4. Reentrancy lock is released after call
    f.env.as_contract(&f.contract_id, || {
        assert!(!shared::guard::is_locked(&f.env));
    });
}

#[test]
fn test_reentrancy_guard_blocks_reentrancy() {
    let f = setup();
    let ms = default_milestones(&f.env);
    let desc = String::from_str(&f.env, "Reentrancy guard test");

    let eng_id = f.client.propose_engagement(&f.mentor, &f.mentee, &desc, &ms, &0, &f.token_id);
    f.client.accept_engagement(&f.mentee, &eng_id);
    f.client.submit_milestone(&f.mentee, &eng_id, &0);

    // Lock is currently free
    f.env.as_contract(&f.contract_id, || {
        assert!(!shared::guard::is_locked(&f.env));
    });

    // Manually acquire lock within the contract instance (simulating an outer guarded call or reentrant context)
    f.env.as_contract(&f.contract_id, || {
        shared::guard::acquire(&f.env);
        assert!(shared::guard::is_locked(&f.env));
    });

    // Attempting to call review_milestone while lock is held must fail / panic with ReentrancyError
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.review_milestone(&f.mentor, &eng_id, &0, &true);
    }));
    assert!(res.is_err(), "Expected reentrant call to review_milestone to be rejected");

    // Attempting to call submit_milestone while lock is held must also fail
    let res_submit = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.submit_milestone(&f.mentee, &eng_id, &1);
    }));
    assert!(res_submit.is_err(), "Expected reentrant call to submit_milestone to be rejected");

    // Release lock within the contract instance
    f.env.as_contract(&f.contract_id, || {
        shared::guard::release(&f.env);
        assert!(!shared::guard::is_locked(&f.env));
    });

    // After release, call succeeds normally
    f.client.review_milestone(&f.mentor, &eng_id, &0, &true);
    f.env.as_contract(&f.contract_id, || {
        assert!(!shared::guard::is_locked(&f.env));
    });
    let ms0 = f.client.get_milestones(&eng_id).get(0).unwrap();
    assert!(ms0.status == MilestoneStatus::Approved);
}

#[test]
fn test_feedback_and_pagination() {
    let f = setup();
    let ms = default_milestones(&f.env);
    let desc = String::from_str(&f.env, "Feedback test");

    let eng_id = f.client.propose_engagement(&f.mentor, &f.mentee, &desc, &ms, &0, &f.token_id);
    f.client.accept_engagement(&f.mentee, &eng_id);
    f.client.submit_milestone(&f.mentee, &eng_id, &0);
    f.client.review_milestone(&f.mentor, &eng_id, &0, &true);
    f.client.submit_milestone(&f.mentee, &eng_id, &1);
    f.client.review_milestone(&f.mentor, &eng_id, &1, &true);

    // Engagement is now completed
    let comment1 = String::from_str(&f.env, "Great mentee, highly dedicated");
    f.client.submit_feedback(&f.mentor, &eng_id, &5, &comment1);

    let comment2 = String::from_str(&f.env, "Learned a massive amount from the mentor");
    f.client.submit_feedback(&f.mentee, &eng_id, &5, &comment2);

    let feedback = f.client.get_feedback(&eng_id);
    assert_eq!(feedback.len(), 2);

    // Paginated test
    let (page, info) = f.client.get_feedback_page(&eng_id, &0, &1);
    assert_eq!(page.len(), 1);
    assert_eq!(info.total, 2);
    assert!(info.has_more);
    assert_eq!(info.next_start, Some(1));
}

#[test]
fn test_issue_certificate() {
    let f = setup();
    let ms = default_milestones(&f.env);
    let desc = String::from_str(&f.env, "Cert test");

    let eng_id = f.client.propose_engagement(&f.mentor, &f.mentee, &desc, &ms, &0, &f.token_id);
    f.client.accept_engagement(&f.mentee, &eng_id);

    // Cannot issue certificate while active
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.issue_certificate(&f.mentor, &eng_id);
    }));
    assert!(res.is_err());

    // Complete milestones
    f.client.submit_milestone(&f.mentee, &eng_id, &0);
    f.client.review_milestone(&f.mentor, &eng_id, &0, &true);
    f.client.submit_milestone(&f.mentee, &eng_id, &1);
    f.client.review_milestone(&f.mentor, &eng_id, &1, &true);

    // Issue certificate
    f.client.issue_certificate(&f.mentee, &eng_id);
    let eng = f.client.get_engagement(&eng_id);
    assert!(eng.certificate_issued);

    // Cannot issue certificate twice
    let res_double = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.issue_certificate(&f.mentor, &eng_id);
    }));
    assert!(res_double.is_err());
}
