// Gas/CPU benchmarks for the `commission_agreement` contract (issue #869).
//
// Run:
//   cargo bench -p commission_agreement --bench commission_agreement_benchmark
//
// BLOCKED ON `main`: `contracts/commission_agreement` does not compile today.
// `cargo check -p commission_agreement` fails with 22 errors — duplicate enum
// discriminants in `errors.rs`, a half-finished rate-limiter integration
// (`DataKey::RateLimiter` / `types::RateLimitKey` referenced but never
// defined), use-after-moves, and a `#[contracterror]` symbol-length error.
// The CI workflow's baseline comment lists this crate as known-broken.
//
// This file is written against the contract's CURRENT API (the same calls its
// tests make) so that the moment the crate compiles again, `cargo bench -p
// commission_agreement` covers these operations out of the box. Nothing here
// can be measured — and no numbers documented — until that pre-existing
// defect is resolved.
//
// Harness is a plain `harness = false` binary (stable toolchain).
//
// Reported per call, after warming the mock host:
//   wall_ns   wall-clock time on this host;
//   cpu       host CPU instructions since the call's .budget.reset_unlimited()`;
//   mem       host memory bytes consumed since the same reset.

extern crate std;

use std::time::Instant;

use soroban_sdk::{
    testutils::Address as _,
    token, vec, Address, Bytes, Env, String, Vec,
};

use commission_agreement::agency::BatchPayment;
use commission_agreement::{CommissionAgreementContract, CommissionAgreementContractClient};

const BUDGET: i128 = 1_000;
const DEADLINE: u32 = 10_000;
const ITERATIONS: usize = 50;

struct Row {
    op: &'static str,
    wall_ns: u128,
    cpu: u64,
    mem: u64,
}

fn measure(env: &Env, op: &'static str, mut f: impl FnMut()) -> Row {
    let mut budget = env.budget();
    // `reset_tracker` only clears the diagnostic tracker; `reset_unlimited`
    // is what zeroes the cumulative cpu/mem counters `*_cost()` read back.
    budget.reset_unlimited();
    let start = Instant::now();
    f();
    let wall = start.elapsed().as_nanos();
    Row {
        op,
        wall_ns: wall,
        cpu: budget.cpu_instruction_cost(),
        mem: budget.memory_bytes_cost(),
    }
}

fn commission_id(env: &Env, i: u32) -> Bytes {
    Bytes::from_slice(env, format!("comm-{i}").as_bytes())
}

fn main() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let client_address = Address::generate(&env);
    let artist_address = Address::generate(&env);
    let agency = Address::generate(&env);
    token::StellarAssetClient::new(&env, &token).mint(&agency, &1_000_000_000);

    let contract_id = env.register_contract(None, CommissionAgreementContract);
    let client = CommissionAgreementContractClient::new(&env, &contract_id);

    let mut rows: Vec<Row> = Vec::new();

    // create_agreement — a fresh commission per call. NOTE: `create_agreement`
    // currently requires `DataKey::RateLimiter` to be configured, and that key
    // does not exist yet, so this measurement captures whatever path the
    // settled rate-limiter integration leaves here.
    for i in 0..ITERATIONS {
        let id = commission_id(&env, i as u32);
        rows.push(measure(&env, "create_agreement", || {
            client.create_agreement(
                &id,
                &client_address,
                &artist_address,
                &String::from_str(&env, "Album artwork"),
                &BUDGET,
                &DEADLINE,
            );
        }));
    }

    // propose_milestone — a milestone proposed against a standing agreement.
    // A single agreement is pre-created (setup is not measured); each call
    // proposes the next milestone with a fresh id and amount.
    {
        let id = commission_id(&env, 999);
        client.create_agreement(
            &id,
            &client_address,
            &artist_address,
            &String::from_str(&env, "Album artwork"),
            &BUDGET,
            &DEADLINE,
        );
        client.accept_agreement(&id);
        for i in 0..ITERATIONS {
            let milestone_id = Bytes::from_slice(&env, format!("ms-{i}").as_bytes());
            rows.push(measure(&env, "propose_milestone", || {
                client.propose_milestone(
                    &id,
                    &milestone_id,
                    &String::from_str(&env, "Sketches"),
                    &10,
                );
            }));
        }
    }

    // approve_milestone — client approval of a proposed milestone.
    {
        let id = commission_id(&env, 998);
        client.create_agreement(
            &id,
            &client_address,
            &artist_address,
            &String::from_str(&env, "Album artwork"),
            &BUDGET,
            &DEADLINE,
        );
        client.accept_agreement(&id);
        let milestone_id = Bytes::from_slice(&env, b"ms-approve");
        client.propose_milestone(
            &id,
            &milestone_id,
            &String::from_str(&env, "Sketches"),
            &10,
        );
        rows.push(measure(&env, "approve_milestone", || {
            client.approve_milestone(&id, &milestone_id);
        }));
    }

    // distribute_batch — agency payout to rostered artists, one line per call.
    {
        client.register_agency(&agency, &String::from_str(&env, "Northlight"), &2_000);
        client.add_artist(&agency, &artist_address, &2_000);
        for _ in 0..ITERATIONS {
            let payments = vec![
                &env,
                BatchPayment {
                    artist: artist_address.clone(),
                    gross_usdc: 100,
                },
            ];
            rows.push(measure(&env, "distribute_batch", || {
                client.distribute_batch(&agency, &token, &payments);
            }));
        }
    }

    print_table(&rows);
}

fn print_table(rows: &[Row]) {
    println!();
    println!("commission_agreement — warm-call budget (n = {ITERATIONS})");
    println!("{:<20} {:>12} {:>12} {:>12}", "op", "wall ns", "cpu", "mem");
    for row in rows {
        println!(
            "{:<20} {:>12} {:>12} {:>12}",
            row.op, row.wall_ns, row.cpu, row.mem
        );
    }
    println!();
}