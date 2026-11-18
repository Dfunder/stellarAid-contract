//! Performance benchmarks for escrow contract operations.
//!
//! Run with:
//!   cargo bench -p escrow-contract
//!
//! These benchmarks measure the gas cost and execution time of core
//! escrow functions using soroban-testbench or the SDK's simulation.

#![cfg(test)]

extern crate test;

use soroban_sdk::{testutils::Address as _, Address, Bytes, Env};
use test::Bencher;

use escrow_contract::{EscrowContract, EscrowContractClient};

fn setup_env() -> (Env, EscrowContractClient<'static>, Address, Address, Address, Bytes) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let client_addr = Address::generate(&env);
    let artist = Address::generate(&env);
    let commission_id = Bytes::from_slice(&env, b"bench-commission-001");
    let config_contract = Address::generate(&env);

    (env, client, admin, client_addr, artist, commission_id)
}

#[bench]
fn bench_create_escrow(b: &mut Bencher) {
    let (_env, client, _admin, client_addr, artist, commission_id) = setup_env();

    b.iter(|| {
        // Creating an escrow involves storage writes and token transfers.
        // The mock-all-auths environment skips signature verification.
        let _ = client.create_escrow(
            &commission_id,
            &client_addr,
            &artist,
            &1_000_i128,
            &Address::generate(&_env),
        );
    });
}

#[bench]
fn bench_open_dispute(b: &mut Bencher) {
    let (env, client, _admin, client_addr, artist, commission_id) = setup_env();

    // Pre-create a commission to dispute
    let config = Address::generate(&env);
    client.create_escrow(&commission_id, &client_addr, &artist, &1_000_i128, &config);

    b.iter(|| {
        let _ = client.open_dispute(&commission_id, &client_addr);
    });
}

#[bench]
fn bench_release_payment(b: &mut Bencher) {
    let (env, client, admin, client_addr, artist, commission_id) = setup_env();

    let config = Address::generate(&env);
    client.create_escrow(&commission_id, &client_addr, &artist, &1_000_i128, &config);

    b.iter(|| {
        let _ = client.release_payment(&commission_id, &config);
    });
}

#[bench]
fn bench_refund_client(b: &mut Bencher) {
    let (env, client, admin, client_addr, artist, commission_id) = setup_env();

    let config = Address::generate(&env);
    client.create_escrow(&commission_id, &client_addr, &artist, &1_000_i128, &config);

    b.iter(|| {
        let _ = client.refund_client(&commission_id, &config);
    });
}

#[bench]
fn bench_get_escrow(b: &mut Bencher) {
    let (env, client, _admin, client_addr, artist, commission_id) = setup_env();

    let config = Address::generate(&env);
    client.create_escrow(&commission_id, &client_addr, &artist, &1_000_i128, &config);

    b.iter(|| {
        let _ = client.get_escrow(&commission_id);
    });
}
