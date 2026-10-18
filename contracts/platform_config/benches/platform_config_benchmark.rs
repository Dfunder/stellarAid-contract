// Gas/CPU benchmarks for the `platform_config` contract (issue #819).
//
// Run:
//   cargo bench -p platform_config --bench platform_config_benchmark
//
// Plain `harness = false` binary (stable toolchain), matching the pattern used
// by the other contract benchmarks in this workspace.
//
// Reported per call, after warming the mock host:
//   wall_ns   wall-clock time on this host;
//   cpu       host CPU instructions since the call's `.budget.reset_unlimited()`;
//   mem       host memory bytes consumed since the same reset.

extern crate std;

use std::time::Instant;

use soroban_sdk::{testutils::Address as _, Address, Env};

use platform_config::{PlatformConfigContract, PlatformConfigContractClient};

const ITERATIONS: usize = 50;

struct Row {
    op: &'static str,
    wall_ns: u128,
    cpu: u64,
    mem: u64,
}

fn measure(env: &Env, op: &'static str, mut f: impl FnMut()) -> Row {
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

fn main() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let admin = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let usdc_token = Address::generate(&env);

    let contract_id = env.register_contract(None, PlatformConfigContract);
    let client = PlatformConfigContractClient::new(&env, &contract_id);
    client.initialize(&admin, &300, &platform_wallet, &usdc_token);

    let mut rows: Vec<Row> = Vec::new();

    // set_fee_bps — admin updates the platform fee (in-range value).
    for i in 0..ITERATIONS {
        let fee = 300 + (i as u32 % 100);
        rows.push(measure(&env, "set_fee_bps", || {
            client.set_fee_bps(&fee);
        }));
    }

    // get_config — warm read of the stored configuration.
    for _ in 0..ITERATIONS {
        rows.push(measure(&env, "get_config", || {
            client.get_config();
        }));
    }

    // get_volume — read of a per-payer volume counter (unset default).
    for _ in 0..ITERATIONS {
        let payer = Address::generate(&env);
        rows.push(measure(&env, "get_volume", || {
            client.get_volume(&payer);
        }));
    }

    print_table(&rows);
}

fn print_table(rows: &[Row]) {
    println!();
    println!("platform_config — warm-call budget (n = {ITERATIONS})");
    println!("{:<18} {:>12} {:>12} {:>12}", "op", "wall ns", "cpu", "mem");
    for row in rows {
        println!(
            "{:<18} {:>12} {:>12} {:>12}",
            row.op, row.wall_ns, row.cpu, row.mem
        );
    }
    println!();
}
