//! Bounded pagination for contract read entry points (closes #876).
//!
//! Several contracts expose a `Vec` that grows for the life of the deployment —
//! a campaign registry, a fee-tier table, a per-entity history log. Returning
//! one of those in a single invocation is unbounded by construction: the cost of
//! a read entry point has to be predictable *before* it runs, and a caller who
//! controls `limit` can make it as expensive as they like.
//!
//! This module is the one place in the workspace where that bound is decided.
//! It offers two shapes, because the workspace has two shapes of list:
//!
//! | List shape | Storage | Use |
//! |------------|---------|-----|
//! | Materialised | one ledger entry holding a whole `Vec<T>` | [`paginate`] / [`paginated`] |
//! | Indexed | a count key plus one key per index | [`collect_window`] |
//!
//! Both clamp the caller's `limit` through [`clamp_limit`], both saturate their
//! arithmetic, and both report the same [`PageInfo`] so an off-chain client can
//! page to the end without a separate count call and without ever mistaking a
//! partial page for a complete one.
//!
//! ## Edge cases
//!
//! | Input | Result |
//! |-------|--------|
//! | `start > total` | empty page, `has_more == false`, `next_start == None` |
//! | `limit == 0` | `DEFAULT_LIMIT` items — a zero reads as "caller did not care", not "return nothing" |
//! | `limit > MAX_LIMIT` | clamped to `MAX_LIMIT` |
//! | `start == total` | empty page, not a trap |
//! | `start = u32::MAX` | `start` saturates and is clamped to `total`; no overflow |
//! | empty list | empty page, `has_more == false` |
//!
//! Nothing here can trap and nothing here can return more than [`MAX_LIMIT`]
//! items, so it is safe to call with caller-controlled arguments from inside a
//! view entry point.
//!
//! ## Why `limit == 0` is not an empty page
//!
//! `analytics::get_earnings` and `messaging::get_messages` treat a zero limit as
//! an empty page. This module deliberately does not, and the difference is
//! worth stating. Those two are entry points whose whole contract is "return
//! what the caller asked for, bounded", so a zero there is a cheap, local
//! mistake. Here the `limit` is only applied *after* the list has already been
//! materialised out of storage or walked index by index, so answering a zero
//! with an empty vector would pay the full read cost and then discard the
//! result, while looking to the caller exactly like a legitimately empty page.
//! Rejecting it instead would force every contract to map a shared error into
//! its own enum to accommodate a helper. `DEFAULT_LIMIT` is the boring answer
//! that keeps the cost bounded and the call useful.

use soroban_sdk::{contracttype, Env, IntoVal, TryFromVal, Val, Vec};

/// Largest number of items any single paginated read will return, however large
/// a `limit` the caller passes.
///
/// 100 is the largest cap already declared in the workspace
/// (`messaging::MAX_HISTORY`), so adopting it here bounds every new reader at
/// no more than the most expensive read the platform already ships.
pub const MAX_LIMIT: u32 = 100;

/// Page size substituted for a caller-supplied `limit == 0`.
///
/// 50 is the modal cap in the workspace (`search::MAX_PAGE_SIZE`,
/// `analytics::MAX_EARNING_PAGE`, `audit::MAX_PAGE_SIZE`).
pub const DEFAULT_LIMIT: u32 = 50;

/// Metadata describing one page of a paginated read.
///
/// Deliberately *not* generic. A `Page<T>` would be nicer to hand back whole,
/// but `soroban-sdk`'s contract-spec lowering rejects generic type parameters
/// ("generics unsupported on user-defined types in contract functions"), so a
/// generic page could not appear in a `#[contractimpl]` signature. Keeping the
/// item type out of [`PageInfo`] lets a contract return
/// `(Vec<SomeRecord>, PageInfo)` and stay spec-compatible.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageInfo {
    /// Total number of items behind this page, not just the number returned.
    pub total: u32,
    /// Cursor this page started at, after clamping to `total`.
    pub start: u32,
    /// Number of index positions this page spans, after clamping.
    pub limit: u32,
    /// Number of items actually in this page.
    ///
    /// Less than `limit` when a page spans indices whose storage keys are
    /// missing: an indexed history is append-only and its count never
    /// decreases, so pruned positions are skipped rather than filled.
    pub count: u32,
    /// `true` when at least one item exists past the end of this page.
    pub has_more: bool,
    /// Cursor to pass as the next `start`, or `None` on the last page.
    pub next_start: Option<u32>,
}

// ── Limit and window arithmetic ─────────────────────────────────────────────

/// Clamp a caller-supplied `limit` into the range the platform will serve in one
/// call.
///
/// * `0` becomes [`DEFAULT_LIMIT`]. See the module docs for why a zero is not
///   treated as "return nothing".
/// * Anything above [`MAX_LIMIT`] becomes [`MAX_LIMIT`].
///
/// The result is never `0`, which is what makes it safe: a caller cannot turn a
/// bounded read into an unbounded one by passing a hostile `limit`.
#[inline]
pub fn clamp_limit(limit: u32) -> u32 {
    if limit == 0 {
        DEFAULT_LIMIT
    } else {
        limit.min(MAX_LIMIT)
    }
}

/// The half-open index range `[start, end)` that a bounded read should visit.
///
/// * `start` is clamped down to `total`, so a cursor that has run past the end
///   yields an empty range instead of an out-of-bounds read.
/// * `limit` goes through [`clamp_limit`], so the range spans at most
///   [`MAX_LIMIT`] positions however large the caller asked for.
/// * The addition saturates, so `start = u32::MAX` cannot overflow.
///
/// Pass the returned `end` as the next call's `start` to continue.
#[inline]
pub fn window(total: u32, start: u32, limit: u32) -> (u32, u32) {
    let from = start.min(total);
    let to = from.saturating_add(clamp_limit(limit)).min(total);
    (from, to)
}

/// Build the [`PageInfo`] for a page spanning `[start, end)` of a list of
/// `total` items, of which `count` were actually returned.
///
/// `end` must already be clamped, as [`window`] does. When `end < total` the
/// page is not the last one, and `next_start` is `Some(end)`.
#[inline]
pub fn page_info(total: u32, start: u32, end: u32, count: u32) -> PageInfo {
    let has_more = end < total;
    PageInfo {
        total,
        start,
        limit: end.saturating_sub(start),
        count,
        has_more,
        next_start: if has_more { Some(end) } else { None },
    }
}

// ── Materialised lists ──────────────────────────────────────────────────────

/// The page of `items` starting at `start`, at most `clamp_limit(limit)` long.
///
/// Takes the whole `Vec` and returns a bounded slice of it. Use this for a list
/// that already lives in a single ledger entry, where the read cost is paid
/// before the limit can apply — bounding the *return* here bounds the response
/// size, but it cannot make the underlying read cheaper. See [`collect_window`]
/// for indexed lists, where the bound does bound the read.
pub fn paginate<T>(env: &Env, items: &Vec<T>, start: u32, limit: u32) -> Vec<T>
where
    T: IntoVal<Env, Val> + TryFromVal<Env, Val>,
{
    let total = items.len();
    let (from, to) = window(total, start, limit);
    if from == to {
        return Vec::new(env);
    }
    items.slice(from..to)
}

/// [`paginate`], plus the [`PageInfo`] a client needs to page to the end.
///
/// This is the pair a contract entry point should return:
/// `(Vec<SomeRecord>, PageInfo)`. The tuple is spec-compatible because neither
/// element is generic — see [`PageInfo`].
pub fn paginated<T>(env: &Env, items: &Vec<T>, start: u32, limit: u32) -> (Vec<T>, PageInfo)
where
    T: IntoVal<Env, Val> + TryFromVal<Env, Val>,
{
    let total = items.len();
    let (from, to) = window(total, start, limit);
    let page = if from == to {
        Vec::new(env)
    } else {
        items.slice(from..to)
    };
    let info = page_info(total, from, to, page.len());
    (page, info)
}

// ── Indexed lists ───────────────────────────────────────────────────────────

/// Walk the index range chosen by [`window`] and collect the entries that
/// resolve, plus the [`PageInfo`] for the walk.
///
/// This is the shape the workspace's indexed logs use: a count key plus one key
/// per index, e.g. `DataKey::History(i)` alongside `DataKey::HistoryCount`.
/// `load` is called once with each index in `[start, end)` and returns `None`
/// for an index whose entry is missing.
///
/// At most `clamp_limit(limit)` calls to `load` are made and at most
/// `clamp_limit(limit)` storage reads can result, so this genuinely bounds the
/// read and not just the response.
///
/// `count` may come out below the window size: append-only counters are never
/// decremented, so a pruned index stays missing and the page is simply short.
pub fn collect_window<T, F>(
    env: &Env,
    total: u32,
    start: u32,
    limit: u32,
    mut load: F,
) -> (Vec<T>, PageInfo)
where
    T: IntoVal<Env, Val> + TryFromVal<Env, Val>,
    F: FnMut(u32) -> Option<T>,
{
    let (from, to) = window(total, start, limit);
    let mut page: Vec<T> = Vec::new(env);
    let mut i = from;
    while i < to {
        if let Some(item) = load(i) {
            page.push_back(item);
        }
        i += 1;
    }
    let info = page_info(total, from, to, page.len());
    (page, info)
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Address;
    use std::vec::Vec as StdVec;

    /// `n` distinct `u32`s, `0..n`, so page contents are observable.
    fn nums(env: &Env, n: u32) -> Vec<u32> {
        let mut v: Vec<u32> = Vec::new(env);
        for i in 0..n {
            v.push_back(i);
        }
        v
    }

    /// `soroban_sdk::Vec` -> `std::vec::Vec` so `assert_eq!` can show a diff.
    fn plain(v: &Vec<u32>) -> StdVec<u32> {
        let mut out: StdVec<u32> = StdVec::new();
        for x in v.iter() {
            out.push(x);
        }
        out
    }

    // ── clamp_limit ─────────────────────────────────────────────────────────

    #[test]
    fn clamp_limit_substitutes_a_default_for_zero() {
        // A zero reads as "caller did not care", never as "return nothing".
        assert_eq!(clamp_limit(0), DEFAULT_LIMIT);
    }

    #[test]
    fn clamp_limit_passes_small_limits_through() {
        for n in 1..=DEFAULT_LIMIT {
            assert_eq!(clamp_limit(n), n);
        }
    }

    #[test]
    fn clamp_limit_caps_at_max_limit() {
        assert_eq!(clamp_limit(MAX_LIMIT), MAX_LIMIT);
        assert_eq!(clamp_limit(MAX_LIMIT + 1), MAX_LIMIT);
        assert_eq!(clamp_limit(u32::MAX), MAX_LIMIT);
    }

    #[test]
    fn clamp_limit_never_returns_zero() {
        // The property that makes a clamped limit safe to loop over.
        for n in [0, 1, MAX_LIMIT, u32::MAX] {
            assert!(clamp_limit(n) > 0);
        }
    }

    // ── window ─────────────────────────────────────────────────────────────

    #[test]
    fn window_returns_a_half_open_range() {
        assert_eq!(window(10, 0, 3), (0, 3));
        assert_eq!(window(10, 3, 4), (3, 7));
    }

    #[test]
    fn window_clamps_start_past_the_end() {
        // The documented `start > len` edge case: empty range, no trap.
        assert_eq!(window(5, 5, 10), (5, 5));
        assert_eq!(window(5, 6, 10), (5, 5));
        assert_eq!(window(5, u32::MAX, 10), (5, 5));
    }

    #[test]
    fn window_clamps_end_to_total() {
        // Asking for more than exists stops at the end rather than overrunning.
        assert_eq!(window(5, 3, 10), (3, 5));
    }

    #[test]
    fn window_never_spans_more_than_max_limit() {
        for start in [0, 1, 7, u32::MAX] {
            let (from, to) = window(1_000, start, u32::MAX);
            assert!(
                to - from <= MAX_LIMIT,
                "window {}..{} exceeds cap",
                from,
                to
            );
        }
    }

    #[test]
    fn window_does_not_overflow() {
        // start + limit would wrap in a debug build without saturating_add.
        let (from, to) = window(u32::MAX, u32::MAX - 1, u32::MAX);
        assert_eq!(from, u32::MAX - 1);
        assert_eq!(to, u32::MAX);
    }

    #[test]
    fn window_on_an_empty_list_is_always_empty() {
        for start in [0, 1, u32::MAX] {
            assert_eq!(window(0, start, 10), (0, 0));
        }
    }

    // ── page_info ──────────────────────────────────────────────────────────

    #[test]
    fn page_info_reports_more_when_end_is_short_of_total() {
        let info = page_info(10, 0, 3, 3);
        assert_eq!(info.total, 10);
        assert_eq!(info.start, 0);
        assert_eq!(info.limit, 3);
        assert_eq!(info.count, 3);
        assert!(info.has_more);
        assert_eq!(info.next_start, Some(3));
    }

    #[test]
    fn page_info_reports_the_last_page_as_final() {
        let info = page_info(10, 8, 10, 2);
        assert_eq!(info.limit, 2);
        assert_eq!(info.count, 2);
        assert!(!info.has_more);
        assert_eq!(info.next_start, None);
    }

    #[test]
    fn page_info_on_an_empty_list() {
        let info = page_info(0, 0, 0, 0);
        assert_eq!(info.total, 0);
        assert_eq!(info.count, 0);
        assert!(!info.has_more);
        assert_eq!(info.next_start, None);
    }

    // ── paginate ───────────────────────────────────────────────────────────

    #[test]
    fn paginate_returns_the_requested_page() {
        let env = Env::default();
        let items = nums(&env, 5);
        assert_eq!(plain(&paginate(&env, &items, 0, 2)), StdVec::from([0, 1]));
        assert_eq!(plain(&paginate(&env, &items, 2, 2)), StdVec::from([2, 3]));
        // A short final page rather than an error.
        assert_eq!(plain(&paginate(&env, &items, 4, 2)), StdVec::from([4]));
    }

    #[test]
    fn paginate_start_past_the_end_is_empty() {
        let env = Env::default();
        let items = nums(&env, 5);
        for start in [5, 6, u32::MAX] {
            assert!(paginate(&env, &items, start, 2).is_empty());
        }
    }

    #[test]
    fn paginate_on_an_empty_list_is_empty() {
        let env = Env::default();
        let items = nums(&env, 0);
        assert!(paginate(&env, &items, 0, 10).is_empty());
        assert!(paginate(&env, &items, 0, 0).is_empty());
    }

    #[test]
    fn paginate_clamps_an_oversized_limit() {
        let env = Env::default();
        let items = nums(&env, MAX_LIMIT + 20);
        assert_eq!(paginate(&env, &items, 0, u32::MAX).len(), MAX_LIMIT);
    }

    #[test]
    fn paginate_treats_a_zero_limit_as_the_default() {
        let env = Env::default();
        let items = nums(&env, DEFAULT_LIMIT + 10);
        assert_eq!(
            paginate(&env, &items, 0, 0).len(),
            DEFAULT_LIMIT,
            "limit 0 must serve a default page, not an empty one"
        );
    }

    #[test]
    fn paginate_walks_the_whole_list_without_gaps_or_repeats() {
        let env = Env::default();
        let total = 137u32;
        let items = nums(&env, total);
        let mut seen: StdVec<u32> = StdVec::new();
        let mut cursor = 0u32;
        loop {
            let (page, info) = paginated(&env, &items, cursor, 25);
            assert!(page.len() <= MAX_LIMIT);
            for x in page.iter() {
                seen.push(x);
            }
            match info.next_start {
                Some(next) => {
                    assert!(info.has_more);
                    cursor = next;
                }
                None => {
                    assert!(!info.has_more);
                    break;
                }
            }
        }
        assert_eq!(seen.len() as u32, total);
        for (i, x) in seen.iter().enumerate() {
            assert_eq!(*x, i as u32, "item {} out of order", i);
        }
    }

    // ── paginated ──────────────────────────────────────────────────────────

    #[test]
    fn paginated_reports_page_metadata() {
        let env = Env::default();
        let items = nums(&env, 10);
        let (page, info) = paginated(&env, &items, 0, 3);
        assert_eq!(plain(&page), StdVec::from([0, 1, 2]));
        assert_eq!(info.total, 10);
        assert_eq!(info.start, 0);
        assert_eq!(info.limit, 3);
        assert_eq!(info.count, 3);
        assert!(info.has_more);
        assert_eq!(info.next_start, Some(3));
    }

    #[test]
    fn paginated_last_page_has_no_next_start() {
        let env = Env::default();
        let items = nums(&env, 10);
        let (page, info) = paginated(&env, &items, 7, 5);
        assert_eq!(plain(&page), StdVec::from([7, 8, 9]));
        assert_eq!(info.count, 3);
        assert!(!info.has_more);
        assert_eq!(info.next_start, None);
    }

    #[test]
    fn paginated_past_the_end_is_an_empty_final_page() {
        // A client must be able to page to the end without first reading a
        // count: one call past the last page returns empty, not an error.
        let env = Env::default();
        let items = nums(&env, 4);
        let (page, info) = paginated(&env, &items, 4, 10);
        assert!(page.is_empty());
        assert_eq!(info.total, 4);
        assert_eq!(info.count, 0);
        assert!(!info.has_more);
        assert_eq!(info.next_start, None);
    }

    #[test]
    fn paginated_reports_the_effective_limit_not_the_requested_one() {
        let env = Env::default();
        let items = nums(&env, MAX_LIMIT + 50);
        let (_page, info) = paginated(&env, &items, 0, u32::MAX);
        assert_eq!(info.limit, MAX_LIMIT);

        let (_page, info) = paginated(&env, &items, 0, 0);
        assert_eq!(info.limit, DEFAULT_LIMIT);
    }

    // ── collect_window ─────────────────────────────────────────────────────

    #[test]
    fn collect_window_visits_only_the_window() {
        let env = Env::default();
        let mut visited: StdVec<u32> = StdVec::new();
        let (page, info) = collect_window(&env, 100, 10, 5, |i| {
            visited.push(i);
            Some(i)
        });
        assert_eq!(plain(&page), StdVec::from([10, 11, 12, 13, 14]));
        assert_eq!(visited, StdVec::from([10, 11, 12, 13, 14]));
        assert_eq!(info.total, 100);
        assert!(info.has_more);
        assert_eq!(info.next_start, Some(15));
    }

    #[test]
    fn collect_window_calls_load_at_most_once_per_position() {
        // The property that makes this a bound on *work*, not just on output.
        let env = Env::default();
        let mut calls = 0u32;
        let (page, _) = collect_window(&env, 10_000, 0, u32::MAX, |i| {
            calls += 1;
            Some(i)
        });
        assert_eq!(calls, MAX_LIMIT);
        assert_eq!(page.len(), MAX_LIMIT);
    }

    #[test]
    fn collect_window_skips_missing_indices_and_reports_a_short_page() {
        // Append-only counters are never decremented, so a pruned index stays
        // missing: the page is short, and `count` says so.
        let env = Env::default();
        let (page, info) = collect_window(&env, 10, 0, 5, |i| match i {
            0 | 1 | 3 => Some(i),
            _ => None,
        });
        assert_eq!(plain(&page), StdVec::from([0, 1, 3]));
        assert_eq!(info.limit, 5, "the window still spans five positions");
        assert_eq!(info.count, 3, "but only three resolved");
    }

    #[test]
    fn collect_window_past_the_end_does_no_work() {
        let env = Env::default();
        let mut calls = 0u32;
        let (page, info) = collect_window(&env, 5, 99, 10, |i| {
            calls += 1;
            Some(i)
        });
        assert_eq!(calls, 0);
        assert!(page.is_empty());
        assert!(!info.has_more);
        assert_eq!(info.next_start, None);
    }

    #[test]
    fn collect_window_zero_limit_uses_the_default() {
        let env = Env::default();
        let mut calls = 0u32;
        let (page, info) = collect_window(&env, 1_000, 0, 0, |i| {
            calls += 1;
            Some(i)
        });
        assert_eq!(calls, DEFAULT_LIMIT);
        assert_eq!(page.len(), DEFAULT_LIMIT);
        assert_eq!(info.limit, DEFAULT_LIMIT);
    }

    #[test]
    fn collect_window_does_not_overflow_at_the_top_of_the_range() {
        let env = Env::default();
        let (page, _) = collect_window(&env, u32::MAX, u32::MAX - 2, u32::MAX, |i| Some(i));
        assert_eq!(plain(&page), StdVec::from([u32::MAX - 2, u32::MAX - 1]));
    }

    #[test]
    fn collect_window_works_for_non_primitive_item_types() {
        // The bound must not depend on the item type: addresses are the
        // realistic case for a campaign roster or an agency roster.
        let env = Env::default();
        let mut all: StdVec<Address> = StdVec::new();
        for _ in 0..5 {
            all.push(Address::generate(&env));
        }
        let (page, info) = collect_window(&env, all.len() as u32, 1, 2, |i| {
            Some(all[i as usize].clone())
        });
        assert_eq!(page.len(), 2);
        assert_eq!(info.count, 2);
        assert_eq!(page.get(0).unwrap(), all[1]);
        assert_eq!(page.get(1).unwrap(), all[2]);
    }
}
