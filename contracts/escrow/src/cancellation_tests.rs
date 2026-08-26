//! End-to-end tests for cancellation settlement of an escrow (#605).

extern crate std;
use soroban_sdk::{contract, contractimpl, testutils::Address as _, token, Address, Bytes, Env};

use crate::errors::EscrowError;
use crate::storage::CommissionStatus;
use crate::{EscrowContract, EscrowContractClient};

const FEE_BPS: u32 = 500;
const AMOUNT: i128 = 10_000;

/// Minimal stand-in for the platform config contract, exposing just the four
/// getters the escrow reads.
#[contract]
pub struct MockConfig;

#[contractimpl]
impl MockConfig {
    pub fn init(env: Env, admin: Address, usdc: Address, platform_wallet: Address) {
        env.storage().instance().set(&0u32, &admin);
        env.storage().instance().set(&1u32, &usdc);
        env.storage().instance().set(&2u32, &platform_wallet);
    }
    pub fn get_adm(env: Env) -> Address {
        env.storage().instance().get(&0u32).unwrap()
    }
    pub fn get_usdc(env: Env) -> Address {
        env.storage().instance().get(&1u32).unwrap()
    }
    pub fn get_pw(env: Env) -> Address {
        env.storage().instance().get(&2u32).unwrap()
    }
    pub fn get_fee_b(_env: Env) -> u32 {
        FEE_BPS
    }
}

struct Fixture<'a> {
    env: Env,
    escrow: EscrowContractClient<'a>,
    config: Address,
    usdc: Address,
    client: Address,
    artist: Address,
    platform_wallet: Address,
    commission_id: Bytes,
}

fn setup<'a>() -> Fixture<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let usdc = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let artist = Address::generate(&env);
    let platform_wallet = Address::generate(&env);

    token::StellarAssetClient::new(&env, &usdc).mint(&client, &1_000_000);

    let config = env.register_contract(None, MockConfig);
    MockConfigClient::new(&env, &config).init(&admin, &usdc, &platform_wallet);

    let escrow_id = env.register_contract(None, EscrowContract);
    let escrow = EscrowContractClient::new(&env, &escrow_id);

    Fixture {
        commission_id: Bytes::from_slice(&env, b"comm-001"),
        env,
        escrow,
        config,
        usdc,
        client,
        artist,
        platform_wallet,
    }
}

impl Fixture<'_> {
    fn create(&self) {
        self.escrow.create_escrow(
            &self.commission_id,
            &self.client,
            &self.artist,
            &AMOUNT,
            &self.config,
        );
    }

    fn balance(&self, account: &Address) -> i128 {
        token::Client::new(&self.env, &self.usdc).balance(account)
    }
}

#[test]
fn cancellation_pays_the_split_and_charges_fee_only_on_the_artist_share() {
    let f = setup();
    f.create();
    let before = f.balance(&f.client);

    // 40% completion: 4000 to the artist, 6000 refunded.
    f.escrow
        .cancel_escrow(&f.commission_id, &f.config, &4_000, &6_000);

    // 5% platform fee on the artist's 4000 only.
    assert_eq!(f.balance(&f.artist), 3_800);
    assert_eq!(f.balance(&f.platform_wallet), 200);
    assert_eq!(f.balance(&f.client), before + 6_000);
    assert_eq!(
        f.escrow.get_escrow(&f.commission_id).status,
        CommissionStatus::Cancelled
    );
}

#[test]
fn the_escrow_is_drained_exactly() {
    let f = setup();
    f.create();
    let escrow_address = f.escrow.address.clone();
    assert_eq!(f.balance(&escrow_address), AMOUNT);

    f.escrow
        .cancel_escrow(&f.commission_id, &f.config, &3_333, &6_667);
    assert_eq!(f.balance(&escrow_address), 0);
}

#[test]
fn a_split_that_does_not_match_the_escrow_is_rejected() {
    let f = setup();
    f.create();
    for (artist_amount, client_refund) in [(4_000i128, 5_999i128), (4_000, 6_001)] {
        let err = f
            .escrow
            .try_cancel_escrow(&f.commission_id, &f.config, &artist_amount, &client_refund)
            .err()
            .unwrap()
            .unwrap();
        assert_eq!(err, EscrowError::InvalidSplit);
    }
}

#[test]
fn negative_amounts_are_rejected() {
    let f = setup();
    f.create();
    let err = f
        .escrow
        .try_cancel_escrow(&f.commission_id, &f.config, &-1, &10_001)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, EscrowError::InvalidAmount);
}

#[test]
fn a_full_refund_cancellation_pays_no_fee() {
    let f = setup();
    f.create();
    let before = f.balance(&f.client);

    f.escrow
        .cancel_escrow(&f.commission_id, &f.config, &0, &AMOUNT);
    assert_eq!(f.balance(&f.artist), 0);
    assert_eq!(f.balance(&f.platform_wallet), 0);
    assert_eq!(f.balance(&f.client), before + AMOUNT);
}

#[test]
fn a_disputed_escrow_can_be_cancelled() {
    let f = setup();
    f.create();
    f.escrow.open_dispute(&f.commission_id, &f.client);
    f.escrow
        .cancel_escrow(&f.commission_id, &f.config, &5_000, &5_000);
    assert_eq!(
        f.escrow.get_escrow(&f.commission_id).status,
        CommissionStatus::Cancelled
    );
}

#[test]
fn a_settled_escrow_cannot_be_cancelled() {
    let f = setup();
    f.create();
    f.escrow.release_payment(&f.commission_id, &f.config);
    let err = f
        .escrow
        .try_cancel_escrow(&f.commission_id, &f.config, &5_000, &5_000)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, EscrowError::InvalidStatus);
}

#[test]
fn cancellation_is_not_repeatable() {
    let f = setup();
    f.create();
    f.escrow
        .cancel_escrow(&f.commission_id, &f.config, &5_000, &5_000);
    let err = f
        .escrow
        .try_cancel_escrow(&f.commission_id, &f.config, &5_000, &5_000)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, EscrowError::InvalidStatus);
}

#[test]
fn cancelled_status_has_a_stable_discriminant() {
    assert_eq!(CommissionStatus::Cancelled as u32, 5);
    assert_eq!(EscrowError::InvalidSplit as u32, 13);
}
