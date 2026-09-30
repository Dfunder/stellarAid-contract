// Gas/CPU benchmarks for the `reputation` contract (issue #870).
//
// Run:
//   cargo bench -p reputation --bench reputation_benchmark
//
// Plain `harness = false` binary (stable toolchain).
//
// Reported per call, after warming the mock host:
//   wall_ns   wall-clock time on this host;
//   cpu       host CPU instructions since the call's .budget.reset_unlimited()`;
//   mem       host memory bytes consumed since the same reset.
//
// The issue suggested `compute_reputation`, which does not exist in this
// contract; the real ops are benchmarked instead. Each call uses a fresh
// review id and account pair, so every write is real (submit → report →
// moderate progress the same review's lifecycle to the next stage).

extern crate std;

use std::time::Instant;

use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, String};

use reputation::types::{ReportReason, ReviewStatus};
use reputation::{ReputationContract, ReputationContractClient};

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

fn review_id(env: &Env, i: u32) -> Bytes {
    Bytes::from_slice(env, format!("review-{i}").as_bytes())
}

fn main() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, ReputationContract);
    let client = ReputationContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    let mut rows: Vec<Row> = Vec::new();

    // submit_review — unique review, unique reviewer/artist pair per call.
    for i in 0..ITERATIONS {
        let id = review_id(&env, i as u32);
        let artist = Address::generate(&env);
        let reviewer = Address::generate(&env);
        rows.push(measure(&env, "submit_review", || {
            client.submit_review(&id, &artist, &reviewer, &42, &String::from_str(&env, "Thorough and prompt."));
        }));
    }

    // get_review — warm read of a stored review.
    for _ in 0..ITERATIONS {
        let id = review_id(&env, 0);
        rows.push(measure(&env, "get_review", || {
            client.get_review(&id);
        }));
    }

    // report_review — each call reports a distinct, previously active review.
    for i in 0..ITERATIONS {
        let id = review_id(&env, 10_000 + i as u32);
        let artist = Address::generate(&env);
        let reviewer = Address::generate(&env);
        let reporter = Address::generate(&env);
        client.submit_review(&id, &artist, &reviewer, &35, &String::from_str(&env, "fine"));
        rows.push(measure(&env, "report_review", || {
            client.report_review(
                &id,
                &reporter,
                &ReportReason::Spam,
                &String::from_str(&env, "duplicate of review-1"),
            );
        }));
    }

    // moderate_review — admin moderation of a reported review per call.
    for i in 0..ITERATIONS {
        let id = review_id(&env, 20_000 + i as u32);
        let artist = Address::generate(&env);
        let reviewer = Address::generate(&env);
        let reporter = Address::generate(&env);
        client.submit_review(&id, &artist, &reviewer, &35, &String::from_str(&env, "fine"));
        client.report_review(
            &id,
            &reporter,
            &ReportReason::Other,
            &String::from_str(&env, "zoom"),
        );
        rows.push(measure(&env, "moderate_review", || {
            client.moderate_review(
                &id,
                &ReviewStatus::Cleared,
                &String::from_str(&env, "reviewed, ok"),
            );
        }));
    }

    print_table(&rows);
}

fn print_table(rows: &[Row]) {
    println!();
    println!("reputation — warm-call budget (n = {ITERATIONS})");
    println!("{:<18} {:>12} {:>12} {:>12}", "op", "wall ns", "cpu", "mem");
    for row in rows {
        println!(
            "{:<18} {:>12} {:>12} {:>12}",
            row.op, row.wall_ns, row.cpu, row.mem
        );
    }
    println!();
}