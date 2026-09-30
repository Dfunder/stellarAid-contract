// Gas/CPU benchmarks for the `competitions` contract (issue #872).
//
// Run:
//   cargo bench -p competitions --bench competitions_benchmark
//
// This harness is a plain `harness = false` binary so it runs on the
// project's stable toolchain — the `escrow` precedent used nightly-only
// `#[bench]` and therefore could not be registered here.
//
// Reported per call, after warming up the mock host:
//   wall_ns   wall-clock time on this host;
//   cpu       host CPU instructions since the call's .budget.reset_unlimited()`
//             (`soroban_sdk::testutils::Budget::cpu_instruction_cost()`);
//   mem       host memory bytes consumed since the same reset.
// The budget numbers are totals for a single call against one competition with
// one entrant/voter — i.e. the marginal cost of one more vote or submission
// at that point in the lifecycle.
//
// Each measured call uses fresh state keys (unique ids / addresses), so an
// iteration is a real write, never a duplicate short-circuit. Token calls
// (`create_competition` escrow, `distribute_prizes` payouts) go through the
// mock Stellar asset contract, the realistic network cost.

extern crate std;

use std::time::Instant;

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, vec, Address, Bytes, Env, String,
};

use competitions::types::CompetitionRules;
use competitions::{Competitions, CompetitionsClient};

const SUBMISSION_LEDGERS: u32 = 100;
const VOTING_LEDGERS: u32 = 100;
const PRIZE_POOL: i128 = 10_000;
const MIN_REPUTATION: u32 = 10;
const HISTORY_LIMIT: u32 = 50;
const ITERATIONS: usize = 50;

struct Row {
    op: &'static str,
    wall_ns_per_call: u128,
    cpu_per_call: u64,
    mem_per_call: u64,
}

/// Run `f` once and report wall time plus host-budget use for that single
/// call. The client is already warm (contract registered, admin initialized)
/// so nothing but `f` is measured.
fn measure(env: &Env, op: &'static str, mut f: impl FnMut()) -> Row {
    // `reset_tracker` only clears the diagnostic tracker; `reset_unlimited`
    // is what zeroes the cumulative cpu/mem counters `*_cost()` read back.
    let mut budget = env.budget();
    budget.reset_unlimited();
    let start = Instant::now();
    f();
    let wall_ns = start.elapsed().as_nanos();
    Row {
        op,
        wall_ns_per_call: wall_ns,
        cpu_per_call: budget.cpu_instruction_cost(),
        mem_per_call: budget.memory_bytes_cost(),
    }
}

fn rules(env: &Env) -> CompetitionRules {
    CompetitionRules {
        submission_ledgers: SUBMISSION_LEDGERS,
        voting_ledgers: VOTING_LEDGERS,
        max_submissions: 10_000,
        min_reputation: MIN_REPUTATION,
        prize_split_bps: vec![env, 6_000, 4_000],
    }
}

fn competition_id(env: &Env, i: u32) -> Bytes {
    Bytes::from_slice(env, format!("comp-{i}").as_bytes())
}

/// A fresh test host with all signatures mocked and the budget limit lifted —
/// a single run may issue hundreds of contract calls, which the default test
/// budget limit does not allow. `measure` still resets the tracker, so the
/// per-call numbers stay accurate.
fn test_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    env
}

/// A fresh, initialized competitions instance with minted organizer tokens.
struct Fixture {
    env: Env,
    client: CompetitionsClient<'static>,
    token: Address,
    organizer: Address,
}

fn fixture() -> Fixture {
    let env = test_env();
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let organizer = Address::generate(&env);
    token::StellarAssetClient::new(&env, &token).mint(&organizer, &1_000_000_000);

    let contract_id = env.register_contract(None, Competitions);
    let client = CompetitionsClient::new(&env, &contract_id);
    client.initialize(&admin, &HISTORY_LIMIT);

    Fixture {
        env,
        client,
        token,
        organizer,
    }
}

fn main() {
    let f = fixture();
    let env = &f.env;
    let client = &f.client;
    let token = &f.token;
    let organizer = &f.organizer;
    let title = String::from_str(env, "Poster jam");
    let r = rules(env);
    let mut rows: Vec<Row> = Vec::new();

    // initialize — singleton write, so one fresh env per call.
    {
        let env = test_env();
        let admin = Address::generate(&env);
        let contract_id = env.register_contract(None, Competitions);
        let client = CompetitionsClient::new(&env, &contract_id);
        rows.push(measure(&env, "initialize", || {
            client.initialize(&admin, &HISTORY_LIMIT);
        }));
    }

    // create_competition — unique id per call; escrows the pool through the
    // mock token, so the transfer cost is included.
    for i in 0..ITERATIONS {
        let id = competition_id(env, i as u32);
        rows.push(measure(env, "create_competition", || {
            client.create_competition(&id, organizer, token, &title, &PRIZE_POOL, &r);
        }));
    }

    // submit — one open competition, a fresh entrant per call.
    let comp = competition_id(env, 999);
    client.create_competition(&comp, organizer, token, &title, &PRIZE_POOL, &r);
    for _ in 0..ITERATIONS {
        let entrant = Address::generate(env);
        rows.push(measure(env, "submit", || {
            client.submit(&comp, &entrant, &title);
        }));
    }

    // vote — submissions exist and the voting window is open; a fresh
    // (entrant, voter) pair per call, reputation already posted.
    let comp = competition_id(env, 998);
    client.create_competition(&comp, organizer, token, &title, &PRIZE_POOL, &r);
    let entrants: std::vec::Vec<Address> =
        (0..ITERATIONS).map(|_| Address::generate(env)).collect();
    let voters: std::vec::Vec<Address> =
        (0..ITERATIONS).map(|_| Address::generate(env)).collect();
    for (entrant, voter) in entrants.iter().zip(voters.iter()) {
        client.submit(&comp, entrant, &title);
        client.set_reputation(voter, &30);
    }
    env.ledger()
        .with_mut(|l| l.sequence_number += SUBMISSION_LEDGERS + 1);
    for (entrant, voter) in entrants.iter().zip(voters.iter()) {
        rows.push(measure(env, "vote", || {
            client.vote(&comp, voter, entrant);
        }));
    }

    // finalize — one competition per call, already submitted to and voted,
    // now past the voting window so finalize is a real ranking computation.
    for i in 0..ITERATIONS {
        let env = test_env();
        let admin = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin).address();
        let organizer = Address::generate(&env);
        token::StellarAssetClient::new(&env, &token).mint(&organizer, &1_000_000_000);
        let contract_id = env.register_contract(None, Competitions);
        let client = CompetitionsClient::new(&env, &contract_id);
        let r = rules(&env);
        let title = String::from_str(&env, "Poster jam");
        let id = competition_id(&env, 1000 + i as u32);
        client.initialize(&admin, &HISTORY_LIMIT);
        client.create_competition(&id, &organizer, &token, &title, &PRIZE_POOL, &r);
        let entrant = Address::generate(&env);
        let voter = Address::generate(&env);
        client.submit(&id, &entrant, &title);
        client.set_reputation(&voter, &30);
        // Two-stage ledger: open the voting window for `vote`, then close it
        // so `finalize` is a real ranking computation.
        env.ledger()
            .with_mut(|l| l.sequence_number += SUBMISSION_LEDGERS + 1);
        client.vote(&id, &voter, &entrant);
        env.ledger()
            .with_mut(|l| l.sequence_number += VOTING_LEDGERS + 1);
        rows.push(measure(&env, "finalize", || {
            client.finalize(&id);
        }));
    }

    // distribute_prizes — one finalized competition per call, then pay out.
    for i in 0..ITERATIONS {
        let env = test_env();
        let admin = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin).address();
        let organizer = Address::generate(&env);
        token::StellarAssetClient::new(&env, &token).mint(&organizer, &1_000_000_000);
        let contract_id = env.register_contract(None, Competitions);
        let client = CompetitionsClient::new(&env, &contract_id);
        let r = rules(&env);
        let title = String::from_str(&env, "Poster jam");
        let id = competition_id(&env, 2000 + i as u32);
        client.initialize(&admin, &HISTORY_LIMIT);
        client.create_competition(&id, &organizer, &token, &title, &PRIZE_POOL, &r);
        let entrant = Address::generate(&env);
        let voter = Address::generate(&env);
        client.submit(&id, &entrant, &title);
        client.set_reputation(&voter, &30);
        env.ledger()
            .with_mut(|l| l.sequence_number += SUBMISSION_LEDGERS + 1);
        client.vote(&id, &voter, &entrant);
        env.ledger()
            .with_mut(|l| l.sequence_number += VOTING_LEDGERS + 1);
        client.finalize(&id);
        rows.push(measure(&env, "distribute_prizes", || {
            client.distribute_prizes(&id);
        }));
    }

    // get_competition — warm read of stored state (comp-999 exists).
    for _ in 0..ITERATIONS {
        let id = competition_id(env, 999);
        rows.push(measure(env, "get_competition", || {
            client.get_competition(&id);
        }));
    }

    print_table(&rows);
}

fn print_table(rows: &[Row]) {
    println!();
    println!("competitions — warm-call budget (n = {ITERATIONS})");
    println!("{:<20} {:>12} {:>12} {:>12}", "op", "wall ns", "cpu", "mem");
    for row in rows {
        println!(
            "{:<20} {:>12} {:>12} {:>12}",
            row.op, row.wall_ns_per_call, row.cpu_per_call, row.mem_per_call
        );
    }
    println!();
}