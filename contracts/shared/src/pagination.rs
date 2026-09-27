//! Cursor-free pagination helpers for contract list returns (closes #876).
//!
//! Several contracts accumulate a list in a single storage entry (review
//! history, badge history, payment records, roster entries, ...) and return the
//! whole thing from one query. A list with no upper bound therefore has no
//! upper bound on the size of its response, and the response has to fit inside
//! one ledger entry's worth of XDR on the way out and be decoded in full by the
//! caller. [`paginate`] caps the response and hands back a cursor so neither
//! side has to carry the whole list.
//!
//! ## What this bounds, and what it does not
//!
//! **Bounded:** the *return value*. A page holds at most [`MAX_PAGE_SIZE`]
//! records, so the response stays inside `max_contract_data_entry_size_bytes`
//! and the caller decodes a bounded number of records no matter how large the
//! stored list has grown.
//!
//! **Not bounded:** the *read*. The list lives in one `persistent()` entry and
//! `paginate` takes that `Vec` by value, so the entry is fetched and decoded in
//! full before it is sliced — a paged query over a 500-entry list still pays
//! for 500 entries on the read side. Paging the return value is therefore a
//! response-size and client-cost fix, not a read-cost fix.
//!
//! Making the read itself bounded needs a different storage layout: one key per
//! record plus a count, so a page touches only its own keys. That is a
//! migration of the underlying data model rather than a helper, and the
//! contracts where it would pay off most are listed in
//! `docs/PERFORMANCE_TARGETS.md` under "Read cost is not yet bounded".
//!
//! ## Conventions
//!
//! - `start` is a zero-based index into the *unfiltered* list.
//! - `limit` is the maximum number of entries to return. `limit == 0` returns an
//!   empty page rather than "everything" — a caller that wants the whole list
//!   has to say so with an explicit bound, which is the point of the helper.
//! - A `limit` above [`MAX_PAGE_SIZE`] is clamped down to it, so a single
//!   misconfigured caller cannot ask for an unbounded page.
//! - `start` beyond the end of the list is *not* an error. It yields an empty
//!   page with `total` still reported and `has_more == false`, so a client that
//!   walks pages to the end can distinguish "past the end" from "empty list"
//!   without a separate round trip.
//!
//! ## Usage
//!
//! ```ignore
//! let env = Env::default();
//! let all = load_all(&env, &artist);
//! let page = paginate(&env, all, start, limit);
//! let items = page.into_items();
//! let meta = page_info; // PageInfo, safe to return across the contract boundary
//! ```

use soroban_sdk::{contracttype, Env, Vec};

/// Hard ceiling on how many entries a single page may contain, regardless of
/// the `limit` a caller passes. 50 entries keeps a list query's *response*
/// comfortably inside `max_contract_data_entry_size_bytes` while still being
/// large enough to be useful for UI pagination. It does not bound the read; see
/// the module docs.
pub const MAX_PAGE_SIZE: u32 = 50;

/// Fallback page size for [`paginate_clamped`] callers that have no configured
/// page size of their own.
pub const DEFAULT_PAGE_SIZE: u32 = 20;

/// Non-generic page metadata, safe to return across the contract boundary.
///
/// Soroban contract types cannot be generic, so contracts that expose a
/// paginated query return the page items and this metadata as a tuple
/// (`(Vec<T>, PageInfo)`). [`Page::info`] builds it from a generic [`Page`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageInfo {
    /// Index of the first entry on this page, after clamping.
    pub start: u32,
    /// Number of entries actually on this page.
    pub returned: u32,
    /// Total number of entries in the underlying list, before pagination.
    pub total: u32,
    /// Whether entries exist after this page.
    pub has_more: bool,
    /// `start` to pass for the next page, or `None` on the last page. Lets a
    /// client walk to the end without ever computing a count itself.
    pub next_start: Option<u32>,
}

/// One bounded slice of a list, plus the metadata a caller needs to fetch the
/// next slice (or stop).
pub struct Page<T> {
    /// The entries on this page, at most [`MAX_PAGE_SIZE`] of them.
    pub items: Vec<T>,
    /// Index of `items[0]` within the original list.
    pub start: u32,
    /// Total entries in the original list.
    pub total: u32,
}

impl<T> Page<T> {
    /// Number of entries on this page.
    pub fn len(&self) -> u32 {
        self.items.len()
    }

    /// Whether this page carries no entries. Note that an empty page does not
    /// imply an empty list — compare against [`Page::total`].
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Whether entries exist after this page.
    ///
    /// Always `false` for an empty page. An empty page has no cursor to advance
    /// to — returning `true` here would make [`Page::next_start`] hand back the
    /// `start` it was given, and a client walking pages would spin on the same
    /// index forever. That case is reachable whenever a caller passes `limit ==
    /// 0`, so it is a real concern rather than a degenerate one.
    pub fn has_more(&self) -> bool {
        !self.items.is_empty() && self.start.saturating_add(self.items.len()) < self.total
    }

    /// `start` value for the next page, or `None` when this is the last page.
    pub fn next_start(&self) -> Option<u32> {
        if self.has_more() {
            Some(self.start.saturating_add(self.items.len()))
        } else {
            None
        }
    }

    /// Contract-boundary metadata for this page.
    pub fn info(&self) -> PageInfo {
        PageInfo {
            start: self.start,
            returned: self.items.len(),
            total: self.total,
            has_more: self.has_more(),
            next_start: self.next_start(),
        }
    }

    /// Drop the metadata and keep only the page items.
    pub fn into_items(self) -> Vec<T> {
        self.items
    }
}

/// Effective page size for a caller-supplied `limit`.
///
/// `0` means "empty page" rather than "unbounded"; anything above
/// [`MAX_PAGE_SIZE`] is clamped down to it.
pub fn effective_limit(limit: u32) -> u32 {
    limit.min(MAX_PAGE_SIZE)
}

/// Return the page of `items` starting at `start` holding at most `limit`
/// entries.
///
/// `items` is taken by value, which for a caller loading from storage means the
/// whole list has already been read by the time this runs — see the module docs
/// on what that does and does not bound.
///
/// `start` at or past the end of `items` yields an empty page rather than an
/// error, and `limit == 0` yields an empty page for any `start`. See the
/// module docs for the full convention.
pub fn paginate<T>(env: &Env, items: Vec<T>, start: u32, limit: u32) -> Page<T> {
    let total = items.len();
    let start = start.min(total);
    let end = start.saturating_add(effective_limit(limit)).min(total);
    // `start <= end <= total` holds by construction, so the range below is
    // always valid and the empty case is simply `start == end`.
    Page {
        items: if start == end {
            Vec::new(env)
        } else {
            items.slice(start..end)
        },
        start,
        total,
    }
}

/// Like [`paginate`], but a `limit` of `0` falls back to `default` instead of
/// returning an empty page.
///
/// Useful for query entry points where `limit` is optional and the contract has
/// a configured page size, e.g. `paginate_clamped(&env, all, start, limit, 20)`.
/// A `default` of `0` still yields an empty page, so the helper can never
/// silently return an unbounded page. The result is still capped at
/// [`MAX_PAGE_SIZE`].
pub fn paginate_clamped<T>(
    env: &Env,
    items: Vec<T>,
    start: u32,
    limit: u32,
    default: u32,
) -> Page<T> {
    let limit = if limit == 0 { default } else { limit };
    paginate(env, items, start, limit)
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    /// A list of `n` consecutive integers, `[0, 1, ..., n - 1]`.
    fn items(env: &Env, n: u32) -> Vec<u32> {
        let mut v = Vec::new(env);
        let mut i = 0;
        while i < n {
            v.push_back(i);
            i += 1;
        }
        v
    }

    fn env_and_items(n: u32) -> (Env, Vec<u32>) {
        let env = Env::default();
        let v = items(&env, n);
        (env, v)
    }

    fn values(page: &Page<u32>) -> std::vec::Vec<u32> {
        let mut out = std::vec::Vec::new();
        for v in page.items.iter() {
            out.push(v);
        }
        out
    }

    #[test]
    fn first_page_returns_limit_entries() {
        let (env, all) = env_and_items(5);
        let page = paginate(&env, all, 0, 2);
        assert_eq!(page.items.len(), 2);
        assert_eq!(values(&page), std::vec![0, 1]);
        assert_eq!(page.start, 0);
        assert_eq!(page.total, 5);
        assert!(page.has_more());
        assert_eq!(page.next_start(), Some(2));
    }

    #[test]
    fn middle_page_offsets_by_start() {
        let (env, all) = env_and_items(5);
        let page = paginate(&env, all, 2, 2);
        assert_eq!(values(&page), std::vec![2, 3]);
        assert_eq!(page.start, 2);
        assert!(page.has_more());
        assert_eq!(page.next_start(), Some(4));
    }

    #[test]
    fn last_page_is_partial_and_terminates() {
        let (env, all) = env_and_items(5);
        let page = paginate(&env, all, 4, 2);
        assert_eq!(values(&page), std::vec![4]);
        assert_eq!(page.start, 4);
        assert!(!page.has_more());
        assert_eq!(page.next_start(), None);
    }

    #[test]
    fn limit_beyond_end_clamps_to_remaining() {
        let (env, all) = env_and_items(3);
        let page = paginate(&env, all, 1, 100);
        assert_eq!(values(&page), std::vec![1, 2]);
        assert!(!page.has_more());
    }

    #[test]
    fn limit_zero_returns_empty_page() {
        let (env, all) = env_and_items(4);
        let page = paginate(&env, all, 0, 0);
        assert!(page.is_empty());
        assert_eq!(page.len(), 0);
        // `total` is still reported, so a caller can tell "zero page size" apart
        // from "empty list" without another round trip.
        assert_eq!(page.total, 4);
        assert_eq!(page.start, 0);
        // Nothing was returned, so there is no cursor to advance past. If
        // `has_more` were true here, `next_start` would echo `start` back and a
        // client walking pages would loop on the same index indefinitely.
        assert!(!page.has_more());
        assert_eq!(page.next_start(), None);
    }

    #[test]
    fn empty_page_never_hands_back_a_stuck_cursor() {
        // Regression guard: every way of getting an empty page out of a
        // non-empty list must terminate, not just the `limit == 0` case.
        let (env, all) = env_and_items(4);
        for (start, limit) in [(0u32, 0u32), (2, 0), (0, 0), (4, 10), (99, 10)] {
            let page = paginate(&env, all.clone(), start, limit);
            assert!(
                page.is_empty(),
                "start={start} limit={limit} should yield an empty page"
            );
            assert_eq!(
                page.next_start(),
                None,
                "start={start} limit={limit} must not return a stuck cursor"
            );
        }
    }

    #[test]
    fn start_beyond_len_is_empty_not_an_error() {
        let (env, all) = env_and_items(3);
        let page = paginate(&env, all, 99, 2);
        assert!(page.is_empty());
        assert_eq!(page.total, 3);
        // `start` is clamped to the end so callers still see a valid cursor.
        assert_eq!(page.start, 3);
        assert!(!page.has_more());
        assert_eq!(page.next_start(), None);
    }

    #[test]
    fn start_equal_to_len_is_empty() {
        let (env, all) = env_and_items(3);
        let page = paginate(&env, all, 3, 2);
        assert!(page.is_empty());
        assert_eq!(page.start, 3);
        assert_eq!(page.total, 3);
    }

    #[test]
    fn empty_list_is_empty_with_zero_total() {
        let env = Env::default();
        let page = paginate(&env, Vec::new(&env), 0, 10);
        assert!(page.is_empty());
        assert_eq!(page.total, 0);
        assert_eq!(page.start, 0);
        assert!(!page.has_more());
    }

    #[test]
    fn empty_list_with_nonzero_start_is_safe() {
        let env = Env::default();
        let page = paginate(&env, Vec::new(&env), 5, 10);
        assert!(page.is_empty());
        assert_eq!(page.total, 0);
        assert_eq!(page.start, 0);
    }

    #[test]
    fn limit_is_clamped_to_max_page_size() {
        let env = Env::default();
        let all = items(&env, MAX_PAGE_SIZE * 3);
        let page = paginate(&env, all, 0, u32::MAX);
        assert_eq!(page.items.len(), MAX_PAGE_SIZE);
        assert!(page.has_more());
        assert_eq!(page.next_start(), Some(MAX_PAGE_SIZE));
    }

    #[test]
    fn limit_exactly_max_page_size_is_allowed() {
        let env = Env::default();
        let all = items(&env, MAX_PAGE_SIZE * 2);
        let page = paginate(&env, all, 0, MAX_PAGE_SIZE);
        assert_eq!(page.items.len(), MAX_PAGE_SIZE);
    }

    #[test]
    fn max_start_plus_max_limit_does_not_overflow() {
        let (env, all) = env_and_items(4);
        // `start + limit` would overflow u32 without the saturating add.
        let page = paginate(&env, all, 1, u32::MAX);
        assert_eq!(values(&page), std::vec![1, 2, 3]);
        assert!(!page.has_more());
    }

    #[test]
    fn walking_pages_visits_every_entry_exactly_once() {
        let (env, all) = env_and_items(7);
        let mut seen = std::vec::Vec::new();
        let mut cursor = 0u32;
        let mut guard = 0u32;
        loop {
            let page = paginate(&env, all.clone(), cursor, 3);
            for v in values(&page) {
                seen.push(v);
            }
            match page.next_start() {
                Some(next) => cursor = next,
                None => break,
            }
            // A non-zero page size must always advance the cursor; without this
            // guard a regression here would hang instead of failing.
            guard += 1;
            assert!(guard < 10, "pagination did not terminate");
        }
        assert_eq!(seen, std::vec![0, 1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn clamped_uses_default_when_limit_is_zero() {
        let (env, all) = env_and_items(10);
        let page = paginate_clamped(&env, all, 0, 0, 3);
        assert_eq!(values(&page), std::vec![0, 1, 2]);
        assert!(page.has_more());
    }

    #[test]
    fn clamped_still_honours_explicit_limit() {
        let (env, all) = env_and_items(10);
        let page = paginate_clamped(&env, all, 0, 1, 3);
        assert_eq!(values(&page), std::vec![0]);
    }

    #[test]
    fn clamped_is_still_capped_at_max_page_size() {
        let env = Env::default();
        let all = items(&env, MAX_PAGE_SIZE + 10);
        let page = paginate_clamped(&env, all, 0, 0, u32::MAX);
        assert_eq!(page.items.len(), MAX_PAGE_SIZE);
    }

    #[test]
    fn clamped_with_zero_default_returns_empty_page() {
        let (env, all) = env_and_items(10);
        let page = paginate_clamped(&env, all, 0, 0, 0);
        assert!(page.is_empty());
        assert_eq!(page.total, 10);
    }

    #[test]
    fn effective_limit_clamps_and_preserves() {
        assert_eq!(effective_limit(0), 0);
        assert_eq!(effective_limit(1), 1);
        assert_eq!(effective_limit(MAX_PAGE_SIZE), MAX_PAGE_SIZE);
        assert_eq!(effective_limit(MAX_PAGE_SIZE + 1), MAX_PAGE_SIZE);
        assert_eq!(effective_limit(u32::MAX), MAX_PAGE_SIZE);
    }

    #[test]
    fn info_mirrors_page_state() {
        let (env, all) = env_and_items(5);
        let info = paginate(&env, all, 2, 2).info();
        assert_eq!(info.start, 2);
        assert_eq!(info.returned, 2);
        assert_eq!(info.total, 5);
        assert!(info.has_more);
        assert_eq!(info.next_start, Some(4));
    }

    #[test]
    fn info_on_last_page_has_no_next_start() {
        let (env, all) = env_and_items(5);
        let info = paginate(&env, all, 4, 2).info();
        assert_eq!(info.start, 4);
        assert_eq!(info.returned, 1);
        assert!(!info.has_more);
        assert_eq!(info.next_start, None);
    }

    #[test]
    fn into_items_yields_the_page() {
        let (env, all) = env_and_items(5);
        let taken = paginate(&env, all, 1, 2).into_items();
        assert_eq!(taken.len(), 2);
        assert_eq!(taken.get(0), Some(1));
        assert_eq!(taken.get(1), Some(2));
    }
}
