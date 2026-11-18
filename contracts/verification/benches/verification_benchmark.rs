//! Gas / CPU benchmarks for the verification contract (closes #873).
//!
//! Run with:
//!
//! ```text
//! cargo bench -p verification
//! ```
//!
//! ## Why this is a custom-harness binary and not `extern crate test`
//!
//! `contracts/escrow/benches/escrow_benchmark.rs` uses `#[bench]` /
//! `test::Bencher`, which needs a nightly toolchain *and* a `[[bench]]` target
//! registered in `Cargo.toml` to be compiled at all. Neither is set up in this
//! repo, so that file has never actually run. This benchmark instead uses
//! `harness = false`, which works on the stable toolchain the repo pins, and is
//! wired into `Cargo.toml` properly, so `cargo bench -p verification` really
//! does execute.
//!
//! ## What is and is not measured
//!
//! This runs against the in-memory test `Env` with a *native* (non-Wasm)
//! contract, so the numbers it prints are **not** on-chain instruction counts
//! and must not be read as such:
//!
//! - `median_ns` is wall-clock time. It is machine-dependent and therefore
//!   **informational only** — reported so a large relative regression is
//!   visible, never enforced by default.
//! - `events` is the number of contract events the invocation published, taken
//!   as a difference on the test host's event log (see `events_emitted_since`).
//!   This is deterministic for a given build and input, and event bytes are
//!   metered on-chain, so this is the one figure that is safely assertable.
//! - `state` is the length of the append-only list the operation grows
//!   (`History` / `BadgeHistory`). Every append re-reads and rewrites the whole
//!   list, so this is the parameter that decides how an operation's cost scales.
//!   `0` means the operation does not grow a list.
//!
//! For true instruction counts, upgrade to `soroban-sdk` >= 22 and read
//! `env.cost_estimate().resources()` (CPU instructions, read/write entries and
//! bytes) after the invocation, then compare against the per-operation budgets
//! in `docs/PERFORMANCE_TARGETS.md`. The repo pins `soroban-sdk` `21.0.0`, where
//! no public budget API exists, which is why it is not used here.
//!
//! ## Enforcement
//!
//! Nothing here fails a normal `cargo bench` or `cargo test` run. To turn the
//! report into a gate:
//!
//! - `VERIFICATION_BENCH_CHECK=1` — enforce the deterministic event budgets.
//! - `VERIFICATION_BENCH_ENFORCE_NS=1` — *additionally* enforce wall-clock
//!   budgets. Off by default because wall-clock budgets are not portable across
//!   CI hardware.
//!
//! The event budgets are duplicated as plain assertions in
//! `event_budgets_match_benchmark` in `tests/event_budgets.rs`, so a change that
//! silently doubles an operation's event volume is caught by `cargo test`
//! without anyone having to run this binary at all.

use std::time::Instant;

use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    Address, Env, String,
};
use verification::{
    types::{BadgeType, QualityScore},
    Verification, VerificationClient,
};

// ── Fixture parameters ───────────────────────────────────────────────────────

const MIN_SCORE: u32 = 70;
const MIN_WORK_COUNT: u32 = 3;
const UPDATE_INTERVAL: u32 = 1_000;
/// Append-list cap configured at `initialize`. Large enough that the lists grow
/// across a full run, small enough to stay within the contract's own trimming.
const HISTORY_LIMIT: u32 = 8;
const WORK_COUNT: u32 = 5;
const BADGE_VALID_FOR: u32 = 500;

/// Iterations per operation.
const ITERS: u32 = 40;

/// A per-criterion mark that blends to `GOOD_SCORE`, clearing `MIN_SCORE`.
const GOOD_SCORE: u32 = 90;

/// The badge types cycled through by the badge benchmarks, so badge history
/// keeps growing across the whole run instead of saturating one entry.
const BADGE_TYPES: [BadgeType; 4] = [
    BadgeType::PortfolioVerified,
    BadgeType::IdVerified,
    BadgeType::TopRated,
    BadgeType::ProfessionalCertified,
];

// ── Cost budgets ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct Budget {
    /// Maximum contract events the operation may emit. Deterministic.
    max_events: u32,
    /// Wall-clock ceiling in nanoseconds. Only enforced when
    /// `VERIFICATION_BENCH_ENFORCE_NS` is set; set ~20x the observed median so
    /// it catches order-of-magnitude regressions rather than scheduler noise.
    max_median_ns: u64,
}

/// Budgets, one entry per benchmarked operation.
///
/// Every one of these operations publishes exactly one event and never more, so
/// a budget of 1 leaves no room for an accidental second `events().publish`. If
/// a change deliberately adds one, it has to come here on purpose.
const BUDGETS: &[(&str, Budget)] = &[
    (
        "submit_portfolio",
        Budget {
            max_events: 1,
            max_median_ns: 2_000_000,
        },
    ),
    (
        "start_review",
        Budget {
            max_events: 1,
            max_median_ns: 2_000_000,
        },
    ),
    (
        "review_portfolio",
        Budget {
            max_events: 1,
            max_median_ns: 4_000_000,
        },
    ),
    (
        "issue_badge",
        Budget {
            max_events: 1,
            max_median_ns: 4_000_000,
        },
    ),
    (
        "revoke_badge",
        Budget {
            max_events: 1,
            max_median_ns: 4_000_000,
        },
    ),
];

// ── Harness ──────────────────────────────────────────────────────────────────

/// One measured operation.
struct Measurement {
    /// Operation name, as it appears in the report.
    name: &'static str,
    /// Wall-clock samples in nanoseconds, one per iteration.
    samples: Vec<u64>,
    /// Contract events emitted by the operation, taken from the last iteration.
    events: u32,
    /// Length of the append-only list the operation grows, after the run.
    state: u32,
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
}

/// A configured contract instance plus the address the benchmarks drive.
///
/// Note there is no per-iteration fixture: each benchmark runs against one
/// contract for the whole loop. That is deliberate. Every operation except
/// `submit_portfolio` mutates an append-only list, and a fresh contract per
/// iteration would reset that list to empty every time — which would make every
/// measurement report the cheapest possible case and hide exactly the
/// growth this benchmark exists to expose.
struct Fixture {
    env: Env,
    client: VerificationClient<'static>,
    reviewer: Address,
    artist: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Verification);
    let client = VerificationClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let reviewer = Address::generate(&env);
    let artist = Address::generate(&env);
    client.initialize(
        &admin,
        &MIN_SCORE,
        &MIN_WORK_COUNT,
        &UPDATE_INTERVAL,
        &HISTORY_LIMIT,
    );
    client.add_reviewer(&reviewer);
    Fixture {
        env,
        client,
        reviewer,
        artist,
    }
}

fn uri(env: &Env) -> String {
    String::from_str(env, "ipfs://portfolio")
}

fn note(env: &Env) -> String {
    String::from_str(env, "reviewed manually")
}

fn good_quality() -> QualityScore {
    QualityScore {
        originality: GOOD_SCORE,
        technique: GOOD_SCORE,
        consistency: GOOD_SCORE,
        presentation: GOOD_SCORE,
    }
}

/// Move the ledger forward so the contract's freshness/expiry arithmetic has a
/// moving input rather than always reading ledger 0.
fn advance_ledgers(env: &Env, by: u32) {
    env.ledger().with_mut(|li| {
        li.sequence = li.sequence.saturating_add(by);
    });
}

/// Contract events published by one call, given the event-log length from
/// immediately before it.
///
/// `env.events().all()` returns the test host's entire event log rather than
/// just the last invocation, so `env.events().all().len()` on its own would
/// count every call made so far on this `Env` and would climb by one per
/// iteration regardless of what the operation does. Taking the difference is
/// what makes the figure an event count.
fn events_emitted_since(before: usize, env: &Env) -> u32 {
    (env.events().all().len() - before) as u32
}

// ── Benchmarks ───────────────────────────────────────────────────────────────

/// First-time portfolio submission: one persistent write (the portfolio) plus
/// one event. Nothing is appended to a list, so this operation's cost is flat
/// across a run — which is why `state` is `0` and why a fresh artist is needed
/// every iteration (`submit_portfolio` rejects a duplicate portfolio).
fn bench_submit_portfolio() -> Measurement {
    let f = setup();
    let mut samples = Vec::with_capacity(ITERS as usize);
    let mut events = 0;
    let mut i = 0;
    while i < ITERS {
        let artist = Address::generate(&f.env);
        let events_before = f.env.events().all().len();
        let started = Instant::now();
        f.client
            .submit_portfolio(&artist, &uri(&f.env), &WORK_COUNT);
        samples.push(started.elapsed().as_nanos() as u64);
        events = events_emitted_since(events_before, &f.env);
        advance_ledgers(&f.env, 1);
        i += 1;
    }
    Measurement {
        name: "submit_portfolio",
        samples,
        events,
        state: 0,
    }
}

/// Opening a review. Cheaper than `review_portfolio` (it records no verdict and
/// writes no quality scores) but a distinct write path: it flips the portfolio
/// status and appends to `History`, so its cost grows with the artist's
/// revision count too. The untimed `update_portfolio` before each iteration
/// puts the portfolio back into `Draft`, because `start_review` cannot be
/// called twice on the same portfolio.
fn bench_start_review() -> Measurement {
    let f = setup();
    f.client
        .submit_portfolio(&f.artist, &uri(&f.env), &WORK_COUNT);

    let mut samples = Vec::with_capacity(ITERS as usize);
    let mut events = 0;
    let mut i = 0;
    while i < ITERS {
        f.client
            .update_portfolio(&f.artist, &uri(&f.env), &WORK_COUNT);

        let events_before = f.env.events().all().len();
        let started = Instant::now();
        f.client.start_review(&f.reviewer, &f.artist);
        samples.push(started.elapsed().as_nanos() as u64);
        events = events_emitted_since(events_before, &f.env);
        advance_ledgers(&f.env, 1);
        i += 1;
    }
    Measurement {
        name: "start_review",
        samples,
        events,
        state: f.client.get_history(&f.artist).len(),
    }
}

/// Recording a verdict. The dominant cost is `push_history`, which re-reads and
/// rewrites the whole `History` list, so this is the operation whose cost grows
/// with an artist's revision count. The untimed `update_portfolio` +
/// `start_review` pair exists only to put the portfolio back into
/// `UnderReview`; without it the second `review_portfolio` would fail and the
/// benchmark would measure the error path.
fn bench_review_portfolio() -> Measurement {
    let f = setup();
    f.client
        .submit_portfolio(&f.artist, &uri(&f.env), &WORK_COUNT);

    let mut samples = Vec::with_capacity(ITERS as usize);
    let mut events = 0;
    let mut i = 0;
    while i < ITERS {
        f.client
            .update_portfolio(&f.artist, &uri(&f.env), &WORK_COUNT);
        f.client.start_review(&f.reviewer, &f.artist);

        let events_before = f.env.events().all().len();
        let started = Instant::now();
        let score =
            f.client
                .review_portfolio(&f.reviewer, &f.artist, &good_quality(), &note(&f.env));
        samples.push(started.elapsed().as_nanos() as u64);
        // A wrong score would mean the benchmark drifted off the approval path,
        // which is the expensive one; fail loudly rather than reporting numbers
        // for a rejected portfolio.
        assert_eq!(score, GOOD_SCORE, "review_portfolio did not approve");
        events = events_emitted_since(events_before, &f.env);
        advance_ledgers(&f.env, 1);
        i += 1;
    }
    Measurement {
        name: "review_portfolio",
        samples,
        events,
        state: f.client.get_history(&f.artist).len(),
    }
}

/// Issuing or renewing a badge: one badge write, a badge-history append, and a
/// badge-types insert-or-skip. Repeats against one artist so badge history
/// grows; note that `track_badge_type` stops writing after the first
/// iteration for each type, which is the steady-state renewal cost.
fn bench_issue_badge() -> Measurement {
    let f = setup();
    let mut samples = Vec::with_capacity(ITERS as usize);
    let mut events = 0;
    let mut i = 0;
    while i < ITERS {
        let badge_type = BADGE_TYPES[(i as usize) % BADGE_TYPES.len()];
        let events_before = f.env.events().all().len();
        let started = Instant::now();
        f.client.issue_badge(
            &f.reviewer,
            &f.artist,
            &badge_type,
            &BADGE_VALID_FOR,
            &note(&f.env),
        );
        samples.push(started.elapsed().as_nanos() as u64);
        events = events_emitted_since(events_before, &f.env);
        advance_ledgers(&f.env, 1);
        i += 1;
    }
    Measurement {
        name: "issue_badge",
        samples,
        events,
        state: f.client.get_badge_history(&f.artist).len(),
    }
}

/// Revoking a badge. A badge can only be revoked once, so each iteration
/// re-issues the same badge type first (a previously revoked badge can be
/// re-issued to start a fresh one) — untimed, so only the revoke is measured.
fn bench_revoke_badge() -> Measurement {
    let f = setup();
    let mut samples = Vec::with_capacity(ITERS as usize);
    let mut events = 0;
    let mut i = 0;
    while i < ITERS {
        let badge_type = BADGE_TYPES[(i as usize) % BADGE_TYPES.len()];
        f.client.issue_badge(
            &f.reviewer,
            &f.artist,
            &badge_type,
            &BADGE_VALID_FOR,
            &note(&f.env),
        );

        let events_before = f.env.events().all().len();
        let started = Instant::now();
        f.client
            .revoke_badge(&f.reviewer, &f.artist, &badge_type, &note(&f.env));
        samples.push(started.elapsed().as_nanos() as u64);
        events = events_emitted_since(events_before, &f.env);
        advance_ledgers(&f.env, 1);
        i += 1;
    }
    Measurement {
        name: "revoke_badge",
        samples,
        events,
        state: f.client.get_badge_history(&f.artist).len(),
    }
}

// ── Report ───────────────────────────────────────────────────────────────────

fn env_flag(name: &str) -> bool {
    std::env::var(name).map(|v| v == "1").unwrap_or(false)
}

fn main() {
    let measurements = [
        bench_submit_portfolio(),
        bench_start_review(),
        bench_review_portfolio(),
        bench_issue_badge(),
        bench_revoke_badge(),
    ];

    println!("verification contract - gas / CPU benchmarks (#873)");
    println!("soroban-sdk in-memory test Env, native (non-Wasm) contract");
    println!("{ITERS} iterations per operation");
    println!();
    println!(
        "{:<20} {:>6} {:>12} {:>7} {:>6}",
        "operation", "iters", "median_ns", "events", "state"
    );
    for m in &measurements {
        println!(
            "{:<20} {:>6} {:>12} {:>7} {:>6}",
            m.name,
            m.samples.len(),
            m.median_ns(),
            m.events,
            m.state
        );
    }
    println!();
    println!("events: deterministic, metered on-chain, gated by the budgets below");
    println!("state:  length of the append-only list the operation grows (0 = none)");
    println!("median_ns: wall clock, machine-dependent, informational only");
    println!();

    let check = env_flag("VERIFICATION_BENCH_CHECK");
    let enforce_ns = env_flag("VERIFICATION_BENCH_ENFORCE_NS");
    if !check && !enforce_ns {
        println!("enforcement OFF - set VERIFICATION_BENCH_CHECK=1 to gate on budgets");
        return;
    }

    let mut failures: Vec<String> = Vec::new();
    for m in &measurements {
        let budget = match BUDGETS.iter().find(|(name, _)| *name == m.name) {
            Some((_, b)) => *b,
            None => {
                failures.push(format!("{}: no budget entry in BUDGETS", m.name));
                continue;
            }
        };
        if m.events > budget.max_events {
            failures.push(format!(
                "{}: emitted {} events, budget {}",
                m.name, m.events, budget.max_events
            ));
        }
        if enforce_ns && m.median_ns() > budget.max_median_ns {
            failures.push(format!(
                "{}: median {}ns exceeds {}ns",
                m.name,
                m.median_ns(),
                budget.max_median_ns
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
