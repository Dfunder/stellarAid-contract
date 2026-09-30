// Gas/CPU benchmarks for the `subscription` contract (issue #871).
//
// Run:
//   cargo bench -p subscription --bench subscription_benchmark
//
// Plain `harness = false` binary (stable toolchain — the `escrow` precedent
// needed nightly `#[bench]` and could not be registered).
//
// Reported per call, after warming the mock host:
//   wall_ns   wall-clock time on this host;
//   cpu       host CPU instructions since the call's .budget.reset_unlimited()`;
//   mem       host memory bytes consumed since the same reset.
//
// These are lifecycle-correct: `subscribe` charges credit a `deposit` already
// funded, `renew` runs after a period elapses inside the grace window, and
// every call uses a fresh subscriber so the write is a real one.

extern crate std;

use std::time::Instant;

use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger},
    token, vec, Address, Env, String, Symbol,
    Vec as SorobanVec,
};

use subscription::{SubscriptionContract, SubscriptionContractClient};

const GRACE: u32 = 100;
const HISTORY_LIMIT: u32 = 50;
const PERIOD: u32 = 1_000;
const TIER: u32 = 1;
const PRICE: i128 = 100;
const ITERATIONS: usize = 50;

struct Row {
    op: &'static str,
    wall_ns: u128,
    cpu: u64,
    mem: u64,
}

fn measure(env: &Env, op: &'static str, mut f: impl FnMut()) -> Row {
    // `reset_tracker` only clears the diagnostic tracker; `reset_unlimited`
    // is what zeroes the cumulative cpu/mem counters `*_cost()` read back.
    let mut budget = env.budget();
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

/// A fresh, initialized subscription instance with a funded subscriber pool
/// and the benchmark tier created.
struct Fixture {
    env: Env,
    client: SubscriptionContractClient<'static>,
    token: Address,
}

fn fixture() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let contract_id = env.register_contract(None, SubscriptionContract);
    let client = SubscriptionContractClient::new(&env, &contract_id);
    client.initialize(&admin, &token, &GRACE, &HISTORY_LIMIT);
    client.create_tier(&TIER, &String::from_str(&env, "Pro"), &PRICE, &PERIOD, &benefits(&env));

    Fixture { env, client, token }
}

fn benefits(env: &Env) -> SorobanVec<Symbol> {
    vec![env, symbol_short!("feed"), symbol_short!("earlyacc")]
}

/// A fresh address pre-funded with USDC that a deposit can pull from.
fn funded_subscriber(env: &Env, token: &Address) -> Address {
    let subscriber = Address::generate(env);
    token::StellarAssetClient::new(env, token).mint(&subscriber, &10_000);
    subscriber
}

fn main() {
    let f = fixture();
    let env = &f.env;
    let client = &f.client;
    let token = &f.token;
    let mut rows: Vec<Row> = Vec::new();

    // create_tier — fresh tier id per call.
    for i in 0..ITERATIONS {
        let tier_id = 100 + i as u32;
        rows.push(measure(env, "create_tier", || {
            client.create_tier(&tier_id, &String::from_str(env, "A la carte"), &PRICE, &PERIOD, &benefits(env));
        }));
    }

    // deposit — a funded subscriber pulls a top-up into credit.
    for _ in 0..ITERATIONS {
        let subscriber = funded_subscriber(env, token);
        rows.push(measure(env, "deposit", || {
            client.deposit(&subscriber, &500);
        }));
    }

    // subscribe — a funded subscriber with credit already in place.
    for _ in 0..ITERATIONS {
        let subscriber = funded_subscriber(env, token);
        client.deposit(&subscriber, &1_000);
        rows.push(measure(env, "subscribe", || {
            client.subscribe(&subscriber, &TIER, &true);
        }));
    }

    // renew — auto-renewing subscription past its period, still in grace.
    for _ in 0..ITERATIONS {
        let subscriber = funded_subscriber(env, token);
        client.deposit(&subscriber, &1_000);
        client.subscribe(&subscriber, &TIER, &true);
        env.ledger()
            .with_mut(|l| l.sequence_number += PERIOD + 1);
        rows.push(measure(env, "renew", || {
            client.renew(&subscriber);
        }));
        env.ledger().with_mut(|l| l.sequence_number -= PERIOD + 1);
    }

    // withdraw — withdraw against pre-deposited credit.
    for _ in 0..ITERATIONS {
        let subscriber = funded_subscriber(env, token);
        client.deposit(&subscriber, &1_000);
        rows.push(measure(env, "withdraw", || {
            client.withdraw(&subscriber, &200);
        }));
    }

    print_table(&rows);
}

fn print_table(rows: &[Row]) {
    println!();
    println!("subscription — warm-call budget (n = {ITERATIONS})");
    println!("{:<14} {:>12} {:>12} {:>12}", "op", "wall ns", "cpu", "mem");
    for row in rows {
        println!(
            "{:<14} {:>12} {:>12} {:>12}",
            row.op, row.wall_ns, row.cpu, row.mem
        );
    }
    println!();
}