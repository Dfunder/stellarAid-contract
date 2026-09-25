extern crate std;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    vec, Address, Bytes, Env, Vec,
};

use crate::errors::MultiSigError;
use crate::types::ProposalStatus;
use crate::{MultiSigContract, MultiSigContractClient};

const EXPIRY_LEDGERS: u32 = 100;

struct Fixture<'a> {
    env: Env,
    client: MultiSigContractClient<'a>,
    admin: Address,
    creator: Address,
    executor: Address,
    signers: Vec<Address>,
    extra: Vec<Address>,
    proposal_id: Bytes,
}

fn setup<'a>() -> Fixture<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let executor = Address::generate(&env);

    // 2-of-3 is the default shape; `extra` holds two more accounts so the
    // 3-of-5 case can be configured from the same fixture.
    let signers = vec![
        &env,
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    ];
    let extra = vec![&env, Address::generate(&env), Address::generate(&env)];

    let contract_id = env.register_contract(None, MultiSigContract);
    let client = MultiSigContractClient::new(&env, &contract_id);
    client.initialize(&admin);
    client.configure(&admin, &signers, &2, &EXPIRY_LEDGERS);

    Fixture {
        proposal_id: Bytes::from_slice(&env, b"payout-001"),
        env,
        client,
        admin,
        creator,
        executor,
        signers,
        extra,
    }
}

impl Fixture<'_> {
    fn action(&self) -> Bytes {
        Bytes::from_slice(&self.env, b"release-escrow")
    }

    fn open(&self) {
        self.client
            .open_proposal(&self.creator, &self.proposal_id, &self.action());
    }

    fn open_id(&self, id: &Bytes) {
        self.client
            .open_proposal(&self.creator, id, &self.action());
    }

    /// Every signer in `set` approves the default proposal.
    fn approve_all(&self, set: &Vec<Address>) {
        for signer in set.iter() {
            self.client.approve(&self.proposal_id, &signer);
        }
    }

    fn signers_of(&self, count: usize) -> Vec<Address> {
        // `set` fixtures are built with a fixed shape; this keeps the helper
        // total so a caller cannot silently approve fewer accounts than it means
        // to. Panicking in a test is the loudest possible signal.
        assert!(count == 3 || count == 5, "fixture only builds 3- or 5-signer sets");
        let mut out = self.signers.clone();
        for extra in self.extra.iter() {
            out.push_back(extra);
        }
        let mut trimmed: Vec<Address> = Vec::new(&self.env);
        let mut i = 0usize;
        for signer in out.iter() {
            if i < count {
                trimmed.push_back(signer);
            }
            i += 1;
        }
        trimmed
    }

    fn configure(&self, set: &Vec<Address>, threshold: u32, expiry: u32) {
        self.client.configure(&self.admin, set, &threshold, &expiry);
    }

    fn advance(&self, ledgers: u32) {
        self.env
            .ledger()
            .with_mut(|l| l.sequence_number += ledgers);
    }
}

// ── Initialization and configuration ─────────────────────────────────────────

#[test]
fn initialize_records_the_configured_two_of_three() {
    let f = setup();
    let config = f.client.get_config();
    assert_eq!(config.signers.len(), 3);
    assert_eq!(config.threshold, 2);
    assert_eq!(config.expiry_ledgers, EXPIRY_LEDGERS);
}

#[test]
fn double_initialize_is_rejected() {
    let f = setup();
    let err = f.client.try_initialize(&f.admin).err().unwrap().unwrap();
    assert_eq!(err, MultiSigError::AlreadyInitialized);
}

#[test]
fn a_three_of_five_scheme_is_accepted() {
    let f = setup();
    let five = f.signers_of(5);
    f.configure(&five, 3, EXPIRY_LEDGERS);
    let config = f.client.get_config();
    assert_eq!(config.signers.len(), 5);
    assert_eq!(config.threshold, 3);
    assert!(f.client.is_signer(&five.get(4).unwrap()));
}

#[test]
fn configure_before_initialize_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, MultiSigContract);
    let client = MultiSigContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let set = vec![&env, Address::generate(&env), Address::generate(&env)];
    let err = client
        .try_configure(&admin, &set, &2, &EXPIRY_LEDGERS)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, MultiSigError::NotInitialized);
}

#[test]
fn a_threshold_above_the_signer_count_is_rejected() {
    let f = setup();
    let err = f
        .client
        .try_configure(&f.admin, &f.signers, &4, &EXPIRY_LEDGERS)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, MultiSigError::InvalidThreshold);
}

#[test]
fn a_single_signature_threshold_is_rejected() {
    let f = setup();
    // 1-of-3 is not a multi-signature scheme; it is accepted nowhere.
    let err = f
        .client
        .try_configure(&f.admin, &f.signers, &1, &EXPIRY_LEDGERS)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, MultiSigError::InvalidThreshold);
}

#[test]
fn an_empty_signer_set_is_rejected() {
    let f = setup();
    let empty: Vec<Address> = Vec::new(&f.env);
    let err = f
        .client
        .try_configure(&f.admin, &empty, &2, &EXPIRY_LEDGERS)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, MultiSigError::InvalidSigners);
}

#[test]
fn a_duplicated_signer_is_rejected() {
    let f = setup();
    let dup = vec![
        &f.env,
        f.signers.get(0).unwrap(),
        f.signers.get(0).unwrap(),
        f.signers.get(2).unwrap(),
    ];
    let err = f
        .client
        .try_configure(&f.admin, &dup, &2, &EXPIRY_LEDGERS)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, MultiSigError::InvalidSigners);
}

#[test]
fn a_zero_or_oversized_expiry_is_rejected() {
    let f = setup();
    for bad in [0u32, 120_961] {
        let err = f
            .client
            .try_configure(&f.admin, &f.signers, &2, &bad)
            .err()
            .unwrap()
            .unwrap();
        assert_eq!(err, MultiSigError::InvalidExpiry);
    }
}

#[test]
fn a_non_admin_cannot_reconfigure() {
    let f = setup();
    let outsider = Address::generate(&f.env);
    let err = f
        .client
        .try_configure(&outsider, &f.signers, &2, &EXPIRY_LEDGERS)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, MultiSigError::Unauthorized);
}

// ── Opening ──────────────────────────────────────────────────────────────────

#[test]
fn opening_a_proposal_starts_it_pending_with_no_signatures() {
    let f = setup();
    f.open();
    let proposal = f.client.get_proposal(&f.proposal_id);
    assert_eq!(proposal.status, ProposalStatus::Pending);
    assert_eq!(proposal.signature_count, 0);
    assert_eq!(proposal.creator, f.creator);
    assert_eq!(proposal.expires_ledger, f.env.ledger().sequence() + EXPIRY_LEDGERS);
    assert_eq!(f.client.get_proposal_count(), 1);
}

#[test]
fn a_duplicate_proposal_id_is_rejected() {
    let f = setup();
    f.open();
    let err = f
        .client
        .try_open_proposal(&f.creator, &f.proposal_id, &f.action())
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, MultiSigError::ProposalExists);
}

#[test]
fn an_empty_action_payload_is_rejected() {
    let f = setup();
    let err = f
        .client
        .try_open_proposal(&f.creator, &f.proposal_id, &Bytes::new(&f.env))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, MultiSigError::InvalidAction);
}

// ── Collecting signatures ────────────────────────────────────────────────────

#[test]
fn one_signature_of_two_does_not_meet_the_threshold() {
    let f = setup();
    f.open();
    f.client.approve(&f.proposal_id, &f.signers.get(0).unwrap());
    assert_eq!(f.client.get_signature_count(&f.proposal_id), 1);
    assert!(f.client.has_approved(&f.proposal_id, &f.signers.get(0).unwrap()));
    let err = f
        .client
        .try_finalize(&f.executor, &f.proposal_id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, MultiSigError::ThresholdNotMet);
    // A failed finalize must leave the proposal open, not promote it.
    assert_eq!(
        f.client.get_proposal(&f.proposal_id).status,
        ProposalStatus::Pending
    );
    assert!(!f.client.is_approved(&f.proposal_id));
}

#[test]
fn two_of_three_signatures_approve_the_proposal() {
    let f = setup();
    f.open();
    f.client.approve(&f.proposal_id, &f.signers.get(0).unwrap());
    f.client.approve(&f.proposal_id, &f.signers.get(1).unwrap());

    assert_eq!(
        f.client.finalize(&f.executor, &f.proposal_id),
        ProposalStatus::Approved
    );
    assert!(f.client.is_approved(&f.proposal_id));
    assert_eq!(
        f.client.get_proposal(&f.proposal_id).status,
        ProposalStatus::Approved
    );
}

#[test]
fn all_three_signatures_still_approve_a_two_of_three_scheme() {
    let f = setup();
    f.open();
    f.approve_all(&f.signers);
    assert_eq!(f.client.get_signature_count(&f.proposal_id), 3);
    assert_eq!(
        f.client.finalize(&f.executor, &f.proposal_id),
        ProposalStatus::Approved
    );
}

#[test]
fn three_of_five_needs_exactly_three_distinct_signers() {
    let f = setup();
    let five = f.signers_of(5);
    f.configure(&five, 3, EXPIRY_LEDGERS);
    f.open();

    f.client.approve(&f.proposal_id, &five.get(0).unwrap());
    f.client.approve(&f.proposal_id, &five.get(1).unwrap());
    let err = f
        .client
        .try_finalize(&f.executor, &f.proposal_id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, MultiSigError::ThresholdNotMet);

    f.client.approve(&f.proposal_id, &five.get(2).unwrap());
    assert_eq!(
        f.client.finalize(&f.executor, &f.proposal_id),
        ProposalStatus::Approved
    );
}

#[test]
fn a_non_signer_cannot_approve() {
    let f = setup();
    f.open();
    let outsider = Address::generate(&f.env);
    let err = f
        .client
        .try_approve(&f.proposal_id, &outsider)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, MultiSigError::NotASigner);
    assert_eq!(f.client.get_signature_count(&f.proposal_id), 0);
}

#[test]
fn a_repeated_signature_is_rejected_and_does_not_inflate_the_count() {
    let f = setup();
    f.open();
    let signer = f.signers.get(0).unwrap();
    f.client.approve(&f.proposal_id, &signer);
    let err = f
        .client
        .try_approve(&f.proposal_id, &signer)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, MultiSigError::DuplicateSigner);
    assert_eq!(f.client.get_signature_count(&f.proposal_id), 1);
}

#[test]
fn signatures_are_bound_to_their_own_proposal() {
    let f = setup();
    let other = Bytes::from_slice(&f.env, b"payout-002");
    f.open();
    f.open_id(&other);
    let signer = f.signers.get(0).unwrap();
    f.client.approve(&f.proposal_id, &signer);

    // The same account approving a different action starts from zero.
    assert!(!f.client.has_approved(&other, &signer));
    assert_eq!(f.client.get_approval(&other, &signer), None);
    assert_eq!(
        f.client.get_approval(&f.proposal_id, &signer),
        Some(f.env.ledger().sequence())
    );
}

#[test]
fn approving_an_unknown_proposal_is_rejected() {
    let f = setup();
    let err = f
        .client
        .try_approve(&f.proposal_id, &f.signers.get(0).unwrap())
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, MultiSigError::ProposalNotFound);
}

#[test]
fn a_closed_proposal_cannot_be_signed_again() {
    let f = setup();
    f.open();
    f.client.approve(&f.proposal_id, &f.signers.get(0).unwrap());
    f.client.approve(&f.proposal_id, &f.signers.get(1).unwrap());
    f.client.finalize(&f.executor, &f.proposal_id);

    let err = f
        .client
        .try_approve(&f.proposal_id, &f.signers.get(2).unwrap())
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, MultiSigError::ProposalClosed);
    let err = f
        .client
        .try_finalize(&f.executor, &f.proposal_id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, MultiSigError::ProposalClosed);
}

// ── Expiry ───────────────────────────────────────────────────────────────────

#[test]
fn signatures_are_refused_once_the_window_has_elapsed() {
    let f = setup();
    f.open();
    f.advance(EXPIRY_LEDGERS);
    let err = f
        .client
        .try_approve(&f.proposal_id, &f.signers.get(0).unwrap())
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, MultiSigError::ProposalExpired);
}

#[test]
fn a_partially_signed_set_expires_instead_of_being_approved() {
    let f = setup();
    f.open();
    f.client.approve(&f.proposal_id, &f.signers.get(0).unwrap());
    f.advance(EXPIRY_LEDGERS);

    let err = f
        .client
        .try_finalize(&f.executor, &f.proposal_id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, MultiSigError::ProposalExpired);
    assert_eq!(
        f.client.get_proposal(&f.proposal_id).status,
        ProposalStatus::Expired
    );
    assert!(!f.client.is_approved(&f.proposal_id));
}

#[test]
fn a_signature_is_still_valid_just_before_the_window_closes() {
    let f = setup();
    f.open();
    f.advance(EXPIRY_LEDGERS - 1);
    f.client.approve(&f.proposal_id, &f.signers.get(0).unwrap());
    f.client.approve(&f.proposal_id, &f.signers.get(1).unwrap());
    assert_eq!(
        f.client.finalize(&f.executor, &f.proposal_id),
        ProposalStatus::Approved
    );
}

#[test]
fn an_expired_proposal_is_terminal() {
    let f = setup();
    f.open();
    f.advance(EXPIRY_LEDGERS);

    // Past the window nothing new may be signed, and finalizing only marks the
    // proposal expired.
    let err = f
        .client
        .try_approve(&f.proposal_id, &f.signers.get(0).unwrap())
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, MultiSigError::ProposalExpired);
    let err = f
        .client
        .try_finalize(&f.executor, &f.proposal_id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, MultiSigError::ProposalExpired);
    assert_eq!(
        f.client.get_proposal(&f.proposal_id).status,
        ProposalStatus::Expired
    );

    // Once expired the proposal is closed: neither signing nor finalizing can
    // revive it, even on a much later ledger.
    f.advance(EXPIRY_LEDGERS);
    let err = f
        .client
        .try_approve(&f.proposal_id, &f.signers.get(0).unwrap())
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, MultiSigError::ProposalClosed);
    let err = f
        .client
        .try_finalize(&f.executor, &f.proposal_id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, MultiSigError::ProposalClosed);
}

// ── Error code stability ─────────────────────────────────────────────────────

/// The `#[contracterror]` discriminants are part of the on-chain ABI; pinning
/// them means a renumbering fails the build instead of silently changing what an
/// SDK decodes.
#[test]
fn error_codes_are_stable() {
    assert_eq!(MultiSigError::NotInitialized as u32, 1);
    assert_eq!(MultiSigError::AlreadyInitialized as u32, 2);
    assert_eq!(MultiSigError::Unauthorized as u32, 3);
    assert_eq!(MultiSigError::InvalidSigners as u32, 4);
    assert_eq!(MultiSigError::InvalidThreshold as u32, 5);
    assert_eq!(MultiSigError::InvalidExpiry as u32, 6);
    assert_eq!(MultiSigError::ProposalNotFound as u32, 7);
    assert_eq!(MultiSigError::ProposalExists as u32, 8);
    assert_eq!(MultiSigError::NotASigner as u32, 9);
    assert_eq!(MultiSigError::DuplicateSigner as u32, 10);
    assert_eq!(MultiSigError::ThresholdNotMet as u32, 11);
    assert_eq!(MultiSigError::ProposalExpired as u32, 12);
    assert_eq!(MultiSigError::ProposalClosed as u32, 13);
    assert_eq!(MultiSigError::InvalidAction as u32, 14);
}
