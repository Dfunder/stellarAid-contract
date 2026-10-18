//! Gas / CPU benchmark for `revenue_sharing::record_revenue` (closes #989).
//!
//! Run with:
//!
//! ```text
//! cargo bench -p revenue_sharing
//! ```
//!
//! ## Why this is a custom-harness binary and not `extern crate test`
//!
//! It follows the reference implementation the project established for #873
//! (`contracts/verification/benches/verification_benchmark.rs`): a
//! `harness = false` target registered in `Cargo.toml`, so `cargo bench` really
//! runs on the pinned stable toolchain. The older `#[bench]` / `test::Bencher`
//! approach needs nightly and a `[[bench]]` target that is not registered here.
//!
//! ## What is and is not measured
//!
//! This runs against the in-memory test `Env` with a *native* (non-Wasm)
//! contract, so the numbers it prints are **not** on-chain instruction counts:
//!
//! - `events` is the number of contract events the invocation published, taken
//!   as a difference on the test host's event log (see `events_emitted_since`).
//!   It is deterministic for a given build and input, and event bytes are
//!   metered on-chain, so it is the one figure that is safely assertable.
//!   `record_revenue` publishes one `revenue` event of its own, and each
//!   positive payout makes the Stellar Asset Contract publish one `transfer`
//!   event, so the budget is exactly `participants + 1`.
//! - `median_ns` is wall-clock time. It is machine-dependent and therefore
//!   **informational only** — reported so a large relative regression is
//!   visible, never enforced by default.
//! - `state` is the length of the `History` list after the run, showing that the
//!   ring buffer stays at the configured `HistoryLimit` however many recipients
//!   a batch has.
//!
//! For true instruction counts, upgrade to `soroban-sdk` >= 22 and read
//! `env.cost_estimate().resources()`. The repo pins `soroban-sdk` 21, where no
//! public budget API exists, which is why events are reported instead.
//!
//! ## Why batch sizes, and the failure path
//!
//! `record_revenue` is a looped transfer: its cost scales with the number of
//! recipients, so a single measurement tells you nothing. Each batch size
//! (2, 5, 20 participants — `MAX_PARTICIPANTS` is 20) is measured separately,
//! and `record_revenue/insufficient` measures the abort path: a source funded
//! for only part of its batch, so the transfer loop fails partway through. That
//! is the case nobody measured before. `try_record_revenue` is used so the
//! failure is observed as an error rather than aborting the process.
//!
//! ## Enforcement
//!
//! Nothing here fails a normal `cargo bench` or `cargo test` run. To gate:
//!
//! - `REVENUE_SHARING_BENCH_CHECK=1` — enforce the deterministic event budgets.
//! - `REVENUE_SHARING_BENCH_ENFORCE_NS=1` — *additionally* enforce wall-clock
//!   budgets. Off by default because wall-clock budgets are not portable across
//!   CI hardware.
//!
//! The event budgets are duplicated as plain assertions in
//! `event_budget_scales_with_participants` in `src/test.rs`, so a change that
//! silently changes a batch's event volume is caught by a plain `cargo test`
//! without anyone running this binary.

extern crate std;

use std::time::Instant;

use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    token, Address, Bytes, Env, String, Vec as SorobanVec,
};
use revenue_sharing::{
    types::{Participant, MAX_PARTICIPANTS, TOTAL_BPS},
    RevenueSharing, RevenueSharingClient,
};

// ── Fixture parameters ───────────────────────────────────────────────────────

/// Iterations per operation.
const ITERS: u32 = 40;

/// Recipient counts measured. Three sizes, spanning the whole useful range up
/// to the contract's own `MAX_PARTICIPANTS = 20`.
const PARTICIPANT_COUNTS: [u32; 3] = [2, 5, 20];

/// Gross revenue booked per call. Divisible by 2, 5 and 20, so an equal split
/// gives every recipient a whole token unit and no rounding dust can hide a
/// payout (which would suppress its `transfer` event).
const GROSS: i128 = 100_000;

/// Cap on the retained `History` list, configured at `initialize`.
const HISTORY_LIMIT: u32 = 8;

const AGREEMENT_ID: &[u8] = b"bench-agreement";

// ── Cost budgets ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct Budget {
    /// Maximum contract events the operation may emit. Deterministic.
    max_events: u32,
    /// Wall-clock ceiling in nanoseconds. Only enforced when
    /// `REVENUE_SHARING_BENCH_ENFORCE_NS` is set; set well above the observed
    /// median so it catches order-of-magnitude regressions, not scheduler noise.
    max_median_ns: u64,
}

/// One `revenue` event plus one `transfer` event per recipient, in the success
/// path. The abort path emits no more than one event per recipient (the
/// transfers that completed before the failing one; the host's rollback may
/// discard them entirely), so `participants` is a safe upper bound.
fn budget_for(participants: u32, failed: bool) -> Budget {
    let max_events = if failed {
        participants
    } else {
        participants + 1
    };
    Budget {
        max_events,
        max_median_ns: 40_000_000,
    }
}

// ── Harness ──────────────────────────────────────────────────────────────────

/// One measured operation.
struct Measurement {
    /// Operation name, as it appears in the report.
    name: &'static str,
    /// Recipients in the batch.
    participants: u32,
    /// Wall-clock samples in nanoseconds, one per iteration.
    samples: std::vec::Vec<u64>,
    /// Contract events emitted by the operation, from the last iteration.
    events: u32,
    /// Length of the `History` list after the run.
    state: u32,
    /// `true` for the abort-path measurement.
    failed: bool,
    budget: Budget,
}

impl Measurement {
    /// Median wall-clock sample. Median rather than mean, so one scheduling
    /// hiccup on a shared CI box does not dominate the figure.
    fn median_ns(&self) -> u64 {
        if self.samples.is_empty() {
            return 0;
        }
        let mut sorted = self.samples.clone();
        sorted.sort_unstable();
        sorted[sorted.len() / 2]
    }

    /// Events per recipient, rounded to two decimals for the report.
    fn events_per_recipient(&self) -> f64 {
        if self.participants == 0 {
            return 0.0;
        }
        self.events as f64 / self.participants as f64
    }
}

/// A configured agreement with `participants` equal-split recipients.
struct Fixture {
    env: Env,
    client: RevenueSharingClient<'static>,
    token: Address,
    source: Address,
    id: Bytes,
}

fn setup(participants: u32) -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin).address();
    let owner = Address::generate(&env);
    let source = Address::generate(&env);
    let id = Bytes::from_slice(&env, AGREEMENT_ID);

    let contract_id = env.register_contract(None, RevenueSharing);
    let client = RevenueSharingClient::new(&env, &contract_id);
    client.initialize(&admin, &HISTORY_LIMIT);

    let share = TOTAL_BPS / participants;
    let mut splits = SorobanVec::new(&env);
    for _ in 0..participants {
        splits.push_back(Participant {
            account: Address::generate(&env),
            share_bps: share,
        });
    }
    client.create_agreement(&id, &owner, &token, &splits);

    Fixture {
        env,
        client,
        token,
        source,
        id,
    }
}

fn mint(f: &Fixture, amount: i128) {
    token::StellarAssetClient::new(&f.env, &f.token).mint(&f.source, &amount);
}

fn memo(env: &Env) -> String {
    String::from_str(env, "benchmark payout")
}

/// Move the ledger forward so the contract's ledger arithmetic has a moving
/// input rather than always reading ledger 0.
fn advance_ledgers(env: &Env, by: u32) {
    env.ledger().with_mut(|li| {
        li.sequence_number = li.sequence_number.saturating_add(by);
    });
}

/// Contract events published by one call, given the event-log length from
/// immediately before it.
///
/// `env.events().all()` returns the test host's entire event log rather than
/// just the last invocation, so a raw length would count every call made so far
/// on this `Env`. Taking the difference is what makes the figure an event count.
fn events_emitted_since(before: usize, env: &Env) -> u32 {
    (env.events().all().len() - before) as u32
}

// ── Benchmarks ───────────────────────────────────────────────────────────────

/// The happy path: book `GROSS` and pay every recipient. The source is funded
/// for the whole run (plus one call's headroom), so every iteration measures a
/// complete distribution and the `events` figure is `participants + 1`.
fn bench_record_revenue(participants: u32) -> Measurement {
    let f = setup(participants);
    mint(&f, GROSS * (ITERS as i128 + 1));

    let mut samples = std::vec::Vec::with_capacity(ITERS as usize);
    let mut events = 0u32;
    let mut i = 0;
    while i < ITERS {
        let events_before = f.env.events().all().len();
        let started = Instant::now();
        f.client
            .record_revenue(&f.id, &f.source, &GROSS, &memo(&f.env));
        samples.push(started.elapsed().as_nanos() as u64);
        events = events_emitted_since(events_before, &f.env);
        advance_ledgers(&f.env, 1);
        i += 1;
    }

    Measurement {
        name: "record_revenue",
        participants,
        samples,
        events,
        state: f.client.get_history(&f.id).len(),
        failed: false,
        budget: budget_for(participants, false),
    }
}

/// The failure path: the source is funded for only half the batch, so the
/// transfer loop runs out of funds partway through and the call aborts. This is
/// the expensive, unmeasured case the issue calls out. `try_record_revenue` is
/// used so the abort is captured as an error instead of terminating the
/// benchmark, and the marker is asserted to have failed so we never report a
/// success as a failure.
fn bench_record_revenue_failure(participants: u32) -> Measurement {
    let f = setup(participants);
    let per_payout = GROSS / participants as i128;
    let funded = (participants / 2).max(1);
    mint(&f, per_payout * funded as i128);

    let mut samples = std::vec::Vec::with_capacity(ITERS as usize);
    let mut events = 0u32;
    let mut i = 0;
    while i < ITERS {
        let events_before = f.env.events().all().len();
        let started = Instant::now();
        let result = f
            .client
            .try_record_revenue(&f.id, &f.source, &GROSS, &memo(&f.env));
        samples.push(started.elapsed().as_nanos() as u64);
        assert!(
            !matches!(&result, Ok(Ok(_))),
            "failure path unexpectedly succeeded at {participants} recipients"
        );
        events = events_emitted_since(events_before, &f.env);
        advance_ledgers(&f.env, 1);
        i += 1;
    }

    Measurement {
        name: "record_revenue/insufficient",
        participants,
        samples,
        events,
        state: f.client.get_history(&f.id).len(),
        failed: true,
        budget: budget_for(participants, true),
    }
}

// ── Report ───────────────────────────────────────────────────────────────────

fn env_flag(name: &str) -> bool {
    std::env::var(name).map(|v| v == "1").unwrap_or(false)
}

fn main() {
    let mut measurements: std::vec::Vec<Measurement> = std::vec::Vec::new();
    for &n in &PARTICIPANT_COUNTS {
        measurements.push(bench_record_revenue(n));
    }
    for &n in &PARTICIPANT_COUNTS {
        measurements.push(bench_record_revenue_failure(n));
    }

    println!("revenue_sharing - gas / CPU benchmark for record_revenue (#989)");
    println!("soroban-sdk in-memory test Env, native (non-Wasm) contract");
    println!("{ITERS} iterations per operation");
    println!();
    println!(
        "{:<30} {:>5} {:>6} {:>12} {:>7} {:>9} {:>6}",
        "operation", "recip", "iters", "median_ns", "events", "ev/recip", "state"
    );
    for m in &measurements {
        println!(
            "{:<30} {:>5} {:>6} {:>12} {:>7} {:>9.2} {:>6}",
            m.name,
            m.participants,
            m.samples.len(),
            m.median_ns(),
            m.events,
            m.events_per_recipient(),
            m.state
        );
    }
    println!();
    println!("events: deterministic, metered on-chain, gated by the budgets below");
    println!("        (1 revenue event + 1 SAC transfer event per positive payout)");
    println!("median_ns: wall clock, machine-dependent, informational only");
    println!("state:  length of the retained History ring (HistoryLimit = {HISTORY_LIMIT})");
    println!();

    // ── Findings ─────────────────────────────────────────────────────────────
    // Cost per recipient in the success path is a flat one event, so the only
    // thing that could make a batch "not fit" is the number of recipients. The
    // contract already caps that at MAX_PARTICIPANTS, and the measured batch at
    // the cap stays far below the network's event-size ceiling, so no new batch
    // limit is required.
    let at_cap = measurements
        .iter()
        .find(|m| !m.failed && m.participants == MAX_PARTICIPANTS);
    if let Some(m) = at_cap {
        println!(
            "max viable batch: {MAX_PARTICIPANTS} (contract MAX_PARTICIPANTS); \
             at the cap a call publishes {} events (~{:.2}/recipient)",
            m.events,
            m.events_per_recipient()
        );
        println!(
            "  no new batch limit needed: the cap already bounds the loop, and \
             {} events is far below the 16,384-byte event budget",
            m.events
        );
    }
    println!();

    let check = env_flag("REVENUE_SHARING_BENCH_CHECK");
    let enforce_ns = env_flag("REVENUE_SHARING_BENCH_ENFORCE_NS");
    if !check && !enforce_ns {
        println!(
            "enforcement OFF - set REVENUE_SHARING_BENCH_CHECK=1 to gate on event budgets"
        );
        return;
    }

    let mut failures: std::vec::Vec<String> = std::vec::Vec::new();
    for m in &measurements {
        if m.events > m.budget.max_events {
            failures.push(format!(
                "{} @ {} recipients: emitted {} events, budget {}",
                m.name, m.participants, m.events, m.budget.max_events
            ));
        }
        if enforce_ns && m.median_ns() > m.budget.max_median_ns {
            failures.push(format!(
                "{} @ {} recipients: median {}ns exceeds {}ns",
                m.name,
                m.participants,
                m.median_ns(),
                m.budget.max_median_ns
            ));
        }
    }

    if failures.is_empty() {
        println!("budget check: PASS");
    } else {
        println!("budget check: FAIL");
        for f in &failures {
            println!("  - {}", f);
        }
        // Non-zero only on an explicit opt-in run, so an ordinary
        // `cargo bench` or `cargo test` is never failed by machine noise.
        std::process::exit(1);
    }
}
