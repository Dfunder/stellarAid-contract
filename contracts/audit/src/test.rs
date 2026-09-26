extern crate std;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Bytes, Env,
};

use crate::errors::AuditError;
use crate::types::{AuditAction, TxStatus};
use crate::{AuditLog, AuditLogClient, MAX_PAGE_SIZE, MAX_QUERY_SCAN};

const USDC: i128 = 1_000_000;
/// A ledger number comfortably past anything the tests reach.
const FAR_FUTURE: u32 = 1_000_000;

struct Fixture<'a> {
    env: Env,
    client: AuditLogClient<'a>,
    admin: Address,
    artist: Address,
    client_account: Address,
    token: Address,
    reference: Bytes,
}

fn setup<'a>() -> Fixture<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);

    let contract_id = env.register_contract(None, AuditLog);
    let client = AuditLogClient::new(&env, &contract_id);
    client.initialize(&admin);

    Fixture {
        reference: Bytes::from_slice(&env, b"commission-001"),
        env,
        client,
        admin,
        artist: Address::generate(&env),
        client_account: Address::generate(&env),
        token: Address::generate(&env),
    }
}

impl Fixture<'_> {
    fn record(&self, amount: i128, status: TxStatus) -> u32 {
        self.client.record_transaction(
            &self.reference,
            &AuditAction::EscrowCreated,
            &self.client_account,
            &self.artist,
            &amount,
            &self.token,
            &status,
        )
    }

    fn advance(&self, ledgers: u32) {
        self.env
            .ledger()
            .with_mut(|l| l.sequence_number += ledgers);
    }
}

// ── Initialization ───────────────────────────────────────────────────────────

#[test]
fn a_fresh_log_is_empty() {
    let f = setup();
    assert_eq!(f.client.get_entry_count(), 0);
    let err = f.client.try_get_entry(&0).err().unwrap().unwrap();
    assert_eq!(err, AuditError::EntryNotFound);
}

#[test]
fn double_initialize_is_rejected() {
    let f = setup();
    let err = f.client.try_initialize(&f.admin).err().unwrap().unwrap();
    assert_eq!(err, AuditError::AlreadyInitialized);
}

#[test]
fn nothing_can_be_appended_before_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, AuditLog);
    let client = AuditLogClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let err = client
        .try_record_transaction(
            &Bytes::from_slice(&env, b"ref"),
            &AuditAction::EscrowCreated,
            &admin,
            &admin,
            &1,
            &admin,
            &TxStatus::Pending,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AuditError::NotInitialized);
}

#[test]
fn a_negative_amount_is_rejected() {
    let f = setup();
    let err = f
        .client
        .try_record_transaction(
            &f.reference,
            &AuditAction::EscrowCreated,
            &f.client_account,
            &f.artist,
            &-1,
            &f.token,
            &TxStatus::Pending,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AuditError::InvalidAmount);
    assert_eq!(f.client.get_entry_count(), 0);
}

#[test]
fn the_activity_label_cannot_be_used_for_a_value_transfer() {
    let f = setup();
    let err = f
        .client
        .try_record_transaction(
            &f.reference,
            &AuditAction::UserActivity,
            &f.client_account,
            &f.artist,
            &0,
            &f.token,
            &TxStatus::Activity,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AuditError::InvalidAction);
    assert_eq!(f.client.get_entry_count(), 0);
}

// ── Append and read ──────────────────────────────────────────────────────────

#[test]
fn an_appended_entry_carries_its_full_detail() {
    let f = setup();
    let sequence = f.record(USDC, TxStatus::Pending);

    let entry = f.client.get_entry(&sequence);
    assert_eq!(entry.sequence, 0);
    assert_eq!(entry.reference, f.reference);
    assert_eq!(entry.action, AuditAction::EscrowCreated);
    assert_eq!(entry.from, f.client_account);
    assert_eq!(entry.to, f.artist);
    assert_eq!(entry.amount, USDC);
    assert_eq!(entry.token, Some(f.token.clone()));
    assert_eq!(entry.status, TxStatus::Pending);
    assert_eq!(entry.ledger, f.env.ledger().sequence());
    assert_eq!(entry.timestamp, f.env.ledger().timestamp());
    assert_eq!(f.client.get_entry_count(), 1);
}

#[test]
fn sequence_numbers_are_sequential_and_never_reused() {
    let f = setup();
    let first = f.record(1_000, TxStatus::Pending);
    f.advance(10);
    let second = f.record(2_000, TxStatus::Completed);
    assert_eq!(first, 0);
    assert_eq!(second, 1);
    assert_eq!(f.client.get_entry_count(), 2);
}

#[test]
fn a_status_change_appends_rather_than_overwrites() {
    let f = setup();
    let opened = f.record(USDC, TxStatus::Pending);
    f.advance(5);
    let settled = f.record(USDC, TxStatus::Completed);

    // The earlier entry is untouched...
    assert_eq!(f.client.get_entry(&opened).status, TxStatus::Pending);
    // ...and the newest entry is the current state of the reference.
    let current = f.client.get_transaction(&f.reference);
    assert_eq!(current.sequence, settled);
    assert_eq!(current.status, TxStatus::Completed);

    let history = f
        .client
        .get_transaction_history(&f.reference, &0, &MAX_PAGE_SIZE);
    assert_eq!(history.total, 2);
    assert_eq!(history.entries.get(0).unwrap().sequence, opened);
    assert_eq!(history.entries.get(1).unwrap().sequence, settled);
}

#[test]
fn an_unknown_reference_is_reported() {
    let f = setup();
    let err = f
        .client
        .try_get_transaction(&Bytes::from_slice(&f.env, b"nope"))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AuditError::ReferenceNotFound);
}

// ── User activity ────────────────────────────────────────────────────────────

#[test]
fn a_user_logs_their_own_activity() {
    let f = setup();
    let sequence = f.client.log_activity(&f.artist, &f.reference);
    let entry = f.client.get_entry(&sequence);
    assert_eq!(entry.action, AuditAction::UserActivity);
    assert_eq!(entry.status, TxStatus::Activity);
    assert_eq!(entry.from, f.artist);
    assert_eq!(entry.to, f.artist);
    assert_eq!(entry.amount, 0);
    assert_eq!(entry.token, None);
}

#[test]
fn an_activity_entry_is_indexed_under_its_actor_only_once() {
    let f = setup();
    f.client.log_activity(&f.artist, &f.reference);
    let page = f.client.get_by_account(&f.artist, &0, &MAX_PAGE_SIZE);
    assert_eq!(page.total, 1);
    // `from == to`, so the entry is indexed once, not twice.
    assert_eq!(page.entries.len(), 1);
    assert!(f
        .client
        .get_by_account(&f.client_account, &0, &MAX_PAGE_SIZE)
        .entries
        .is_empty());
}

// ── Query by account ─────────────────────────────────────────────────────────

#[test]
fn an_entry_is_indexed_under_both_parties() {
    let f = setup();
    f.record(USDC, TxStatus::Completed);
    let as_recipient = f.client.get_by_account(&f.artist, &0, &MAX_PAGE_SIZE);
    let as_sender = f.client.get_by_account(&f.client_account, &0, &MAX_PAGE_SIZE);
    assert_eq!(as_recipient.total, 1);
    assert_eq!(as_sender.total, 1);
    assert_eq!(as_recipient.entries.get(0).unwrap().sequence, 0);
    assert_eq!(as_sender.entries.get(0).unwrap().sequence, 0);
}

#[test]
fn an_account_query_pages_in_order() {
    let f = setup();
    for i in 0..5 {
        f.advance(1);
        f.client.record_transaction(
            &f.reference,
            &AuditAction::MilestoneApproved,
            &f.client_account,
            &f.artist,
            &100 * (i as i128 + 1),
            &f.token,
            &TxStatus::Completed,
        );
    }
    let first = f.client.get_by_account(&f.artist, &0, &2);
    assert_eq!(first.entries.len(), 2);
    assert_eq!(first.total, 5);
    assert!(first.has_more);
    assert_eq!(first.next_sequence, Some(2));

    let middle = f.client.get_by_account(&f.artist, &2, &2);
    assert_eq!(middle.entries.len(), 2);
    assert!(middle.has_more);

    let tail = f.client.get_by_account(&f.artist, &4, &2);
    assert_eq!(tail.entries.len(), 1);
    assert!(!tail.has_more);
    assert_eq!(tail.next_sequence, None);
}

#[test]
fn a_page_beyond_the_end_is_empty_rather_than_an_error() {
    let f = setup();
    f.record(USDC, TxStatus::Completed);
    let page = f.client.get_by_account(&f.artist, &9, &MAX_PAGE_SIZE);
    assert!(page.entries.is_empty());
    assert!(!page.has_more);
}

// ── Query by status ──────────────────────────────────────────────────────────

#[test]
fn entries_are_indexed_by_status() {
    let f = setup();
    f.record(USDC, TxStatus::Pending);
    f.advance(1);
    f.record(USDC, TxStatus::Completed);
    f.advance(1);
    f.record(USDC, TxStatus::Completed);

    let pending = f.client.get_by_status(&TxStatus::Pending, &0, &MAX_PAGE_SIZE);
    assert_eq!(pending.total, 1);
    assert_eq!(pending.entries.get(0).unwrap().sequence, 0);

    let completed = f
        .client
        .get_by_status(&TxStatus::Completed, &0, &MAX_PAGE_SIZE);
    assert_eq!(completed.total, 2);
    assert_eq!(completed.entries.get(0).unwrap().sequence, 1);
    assert_eq!(completed.entries.get(1).unwrap().sequence, 2);

    assert!(f
        .client
        .get_by_status(&TxStatus::Refunded, &0, &MAX_PAGE_SIZE)
        .entries
        .is_empty());
}

// ── Query by ledger range ────────────────────────────────────────────────────

#[test]
fn a_ledger_range_query_selects_only_entries_inside_the_window() {
    let f = setup();
    f.record(1_000, TxStatus::Pending);
    f.advance(10);
    f.record(2_000, TxStatus::Pending);
    f.advance(10);
    f.record(3_000, TxStatus::Completed);

    let page = f
        .client
        .get_by_ledger_range(&10, &20, &0, &0, &MAX_PAGE_SIZE);
    assert_eq!(page.total, 2);
    assert_eq!(page.entries.get(0).unwrap().sequence, 1);
    assert_eq!(page.entries.get(1).unwrap().sequence, 2);
    assert!(!page.truncated);
    assert_eq!(page.next_sequence, None);
}

#[test]
fn an_inverted_ledger_range_is_rejected() {
    let f = setup();
    let err = f
        .client
        .try_get_by_ledger_range(&20, &10, &0, &0, &MAX_PAGE_SIZE)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, AuditError::InvalidLedgerRange);
}

#[test]
fn a_range_scan_is_bounded_and_reports_its_cursor() {
    let f = setup();
    for _ in 0..(MAX_QUERY_SCAN + 10) {
        f.advance(1);
        f.record(1_000, TxStatus::Completed);
    }
    let page = f
        .client
        .get_by_ledger_range(&0, &FAR_FUTURE, &0, &0, &MAX_PAGE_SIZE);
    // The walk visited at most MAX_QUERY_SCAN entries, so the caller is told the
    // page is partial and where to resume.
    assert!(page.truncated);
    assert_eq!(page.next_sequence, Some(MAX_QUERY_SCAN));
    assert_eq!(page.total, MAX_QUERY_SCAN);
    assert!(page.total > MAX_PAGE_SIZE);
    assert_eq!(page.entries.len(), MAX_PAGE_SIZE);

    let resumed = f.client.get_by_ledger_range(
        &0,
        &FAR_FUTURE,
        &page.next_sequence.unwrap(),
        &0,
        &MAX_PAGE_SIZE,
    );
    assert!(!resumed.truncated);
    assert_eq!(resumed.total, 10);
}

// ── Page-size bounds ─────────────────────────────────────────────────────────

#[test]
fn a_page_size_outside_the_documented_bounds_is_rejected() {
    let f = setup();
    f.record(USDC, TxStatus::Completed);
    for bad in [0u32, MAX_PAGE_SIZE + 1, 1_000] {
        assert_eq!(
            f.client
                .try_get_by_account(&f.artist, &0, &bad)
                .err()
                .unwrap()
                .unwrap(),
            AuditError::InvalidPageSize
        );
        assert_eq!(
            f.client
                .try_get_by_status(&TxStatus::Completed, &0, &bad)
                .err()
                .unwrap()
                .unwrap(),
            AuditError::InvalidPageSize
        );
        assert_eq!(
            f.client
                .try_get_transaction_history(&f.reference, &0, &bad)
                .err()
                .unwrap()
                .unwrap(),
            AuditError::InvalidPageSize
        );
        assert_eq!(
            f.client
                .try_get_by_ledger_range(&0, &FAR_FUTURE, &0, &0, &bad)
                .err()
                .unwrap()
                .unwrap(),
            AuditError::InvalidPageSize
        );
        assert_eq!(
            f.client
                .try_export_audit_log(&0, &FAR_FUTURE, &0, &0, &bad)
                .err()
                .unwrap()
                .unwrap(),
            AuditError::InvalidPageSize
        );
    }
}

#[test]
fn the_maximum_page_size_is_accepted() {
    let f = setup();
    f.record(USDC, TxStatus::Completed);
    let page = f.client.get_by_account(&f.artist, &0, &MAX_PAGE_SIZE);
    assert_eq!(page.page_size, MAX_PAGE_SIZE);
    assert_eq!(page.entries.len(), 1);
}

// ── Export ───────────────────────────────────────────────────────────────────

#[test]
fn an_export_covers_the_window() {
    let f = setup();
    f.record(1_000, TxStatus::Pending);
    f.advance(10);
    f.record(2_000, TxStatus::Completed);

    let export = f
        .client
        .export_audit_log(&0, &20, &0, &0, &MAX_PAGE_SIZE);
    assert_eq!(export.from_ledger, 0);
    assert_eq!(export.to_ledger, 20);
    assert_eq!(export.total, 2);
    assert_eq!(export.entries.len(), 2);
    assert!(!export.truncated);
}

#[test]
fn an_export_of_a_long_log_is_flagged_truncated() {
    let f = setup();
    for _ in 0..(MAX_QUERY_SCAN + 5) {
        f.advance(1);
        f.record(1_000, TxStatus::Completed);
    }
    let export = f
        .client
        .export_audit_log(&0, &FAR_FUTURE, &0, &0, &MAX_PAGE_SIZE);
    assert!(export.truncated);
    assert_eq!(export.total, MAX_QUERY_SCAN);
    assert_eq!(export.entries.len(), MAX_PAGE_SIZE);
}

// ── Immutability ─────────────────────────────────────────────────────────────
//
// The guarantee is the *absence* of mutating entry points, and no runtime test
// can assert that a function does not exist. What a test can show is the
// observable half: entries are written once under a never-reused sequence
// number and nothing a caller can do afterwards changes them.

#[test]
fn stored_entries_never_change_after_further_appends() {
    let f = setup();
    let first = f.record(1_000, TxStatus::Pending);
    let snapshot = f.client.get_entry(&first);
    for _ in 0..5 {
        f.advance(1);
        f.record(2_000, TxStatus::Completed);
    }
    // Reading entry 0 after five more appends returns exactly what was written.
    assert_eq!(f.client.get_entry(&first), snapshot);
    assert_eq!(f.client.get_entry_count(), 6);
    // The newest entry for the reference moved on; the old one did not change.
    assert_eq!(f.client.get_transaction(&f.reference).sequence, 5);
    assert_eq!(f.client.get_entry(&first).sequence, first);
    assert_eq!(f.client.get_entry(&first).status, TxStatus::Pending);
}

#[test]
fn the_only_way_an_entry_changes_status_is_by_appending_a_new_one() {
    let f = setup();
    let opened = f.record(USDC, TxStatus::Pending);
    f.advance(1);
    let failed = f.record(USDC, TxStatus::Failed);
    f.advance(1);
    let refunded = f.record(USDC, TxStatus::Refunded);

    // Three entries, three distinct statuses, and the first is still the first.
    assert_eq!(f.client.get_entry(&opened).status, TxStatus::Pending);
    assert_eq!(f.client.get_entry(&failed).status, TxStatus::Failed);
    assert_eq!(f.client.get_entry(&refunded).status, TxStatus::Refunded);
    assert_eq!(f.client.get_entry_count(), 3);

    let history = f
        .client
        .get_transaction_history(&f.reference, &0, &MAX_PAGE_SIZE);
    assert_eq!(history.total, 3);
    assert_eq!(history.entries.get(0).unwrap().sequence, opened);
    assert_eq!(history.entries.get(2).unwrap().sequence, refunded);
}

// ── Error code stability ─────────────────────────────────────────────────────

#[test]
fn error_codes_are_stable() {
    assert_eq!(AuditError::NotInitialized as u32, 1);
    assert_eq!(AuditError::AlreadyInitialized as u32, 2);
    assert_eq!(AuditError::Unauthorized as u32, 3);
    assert_eq!(AuditError::EntryNotFound as u32, 4);
    assert_eq!(AuditError::ReferenceNotFound as u32, 5);
    assert_eq!(AuditError::InvalidAmount as u32, 6);
    assert_eq!(AuditError::InvalidAction as u32, 7);
    assert_eq!(AuditError::InvalidPageSize as u32, 8);
    assert_eq!(AuditError::InvalidLedgerRange as u32, 9);
}
