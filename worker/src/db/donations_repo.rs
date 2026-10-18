//! Donation persistence for the worker (closes #982).
//!
//! Donation data is **append-only**: `DonationStatus` moves
//! `Pending → Submitted → Confirming → Confirmed` and rows are inserted, never
//! reordered or deleted. That property is what makes a cursor the right
//! pagination key here.
//!
//! # Why not `LIMIT`/`OFFSET`
//!
//! Offset pagination drifts on an append-only table: a donation confirmed
//! between a client's page 1 and page 2 shifts every subsequent row by one, so
//! the client sees one record twice and misses another. On a live donation feed
//! that is not an edge case. A cursor keyed on `(created_at, id)` is stable
//! under concurrent inserts, and `id` is the tiebreak for two donations that
//! share a ledger timestamp.
//!
//! # Cursor format
//!
//! The cursor is **opaque**: the client must pass back exactly the value the
//! previous response returned. Internally it is a hex encoding of
//! `"{created_at}\u{1f}{id}"` (unit-separator, which cannot occur in an
//! ISO-8601 timestamp), but nothing outside this module depends on that shape.
//!
//! # Retention and expiry
//!
//! Cursors are **positional**, not row references: a cursor names the
//! `(created_at, id)` boundary the next page starts after. Deleting or pruning
//! the boundary row does not invalidate it, because the comparison is on the
//! ordered key rather than on the row's existence — the next request simply
//! resumes at the first surviving row after that key. A cursor therefore does
//! not expire while the repository is alive, and a cursor whose boundary predates
//! every retained row yields the earliest retained page (which the caller can
//! detect via `page.total`), rather than an empty page that looks like the end.
//! A malformed cursor is rejected with [`DbError::Storage`] rather than silently
//! treated as "from the beginning".

use std::collections::HashMap;
use std::sync::Mutex;
use thiserror::Error;
use crate::models::donation_status::DonationStatus;

/// Page size used when a caller passes `limit == 0`.
pub const DEFAULT_PAGE_SIZE: u32 = 50;

/// Largest page this repository will return, however large a `limit` is passed.
pub const MAX_PAGE_SIZE: u32 = 100;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("Storage error: {0}")]
    Storage(String),
}

#[derive(Debug, Clone)]
pub struct NewDonation {
    pub tx_hash: String,
    pub campaign_id: String,
    pub donor_address: String,
    pub amount: u64,
    pub status: DonationStatus,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct Donation {
    pub id: u64,
    pub tx_hash: String,
    pub campaign_id: String,
    pub donor_address: String,
    pub amount: u64,
    pub status: DonationStatus,
    pub created_at: String,
}

/// Optional filters that compose with cursor pagination (#982).
///
/// A filter narrows the result set; the cursor remains valid within the
/// *filtered* set, because it is compared against the same `(created_at, id)`
/// ordering that produced it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DonationFilter {
    /// Restrict to a single lifecycle status.
    pub status: Option<DonationStatus>,
    /// Restrict to a single campaign.
    pub campaign_id: Option<String>,
}

/// Page metadata for a donation list response (#982).
///
/// Mirrors the contract-side `PageInfo` shape, with `next_cursor` in place of
/// `next_start`: the client passes `next_cursor` back verbatim and never
/// computes one itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageInfo {
    /// Number of donations behind this page, after filters.
    pub total: u32,
    /// Effective page size, after clamping `limit`.
    pub limit: u32,
    /// Number of donations actually in this page.
    pub count: u32,
    /// `true` when at least one matching donation exists past this page.
    pub has_more: bool,
    /// Opaque cursor to pass as the next request's `cursor`, or `None` on the
    /// last page.
    pub next_cursor: Option<String>,
}

/// One page of donations plus its [`PageInfo`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DonationPage {
    pub donations: Vec<Donation>,
    pub page: PageInfo,
}

/// In-memory donations repository. Replace `inner` with a real DB connection in production.
pub struct DonationsRepo {
    inner: Mutex<(u64, HashMap<String, Donation>)>,
}

impl DonationsRepo {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new((0, HashMap::new())),
        }
    }

    /// Save a donation. Idempotent: returns existing record if `tx_hash` already exists.
    pub fn save_donation(&self, donation: &NewDonation) -> Result<Donation, DbError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|e| DbError::Storage(e.to_string()))?;
        let (next_id, ref mut map) = *guard;

        if let Some(existing) = map.get(&donation.tx_hash) {
            return Ok(existing.clone());
        }

        let id = next_id + 1;
        guard.0 = id;
        let record = Donation {
            id,
            tx_hash: donation.tx_hash.clone(),
            campaign_id: donation.campaign_id.clone(),
            donor_address: donation.donor_address.clone(),
            amount: donation.amount,
            status: donation.status.clone(),
            created_at: donation.created_at.clone(),
        };
        guard.1.insert(record.tx_hash.clone(), record.clone());
        Ok(record)
    }

    pub fn find_by_tx_hash(&self, tx_hash: &str) -> Result<Option<Donation>, DbError> {
        let guard = self
            .inner
            .lock()
            .map_err(|e| DbError::Storage(e.to_string()))?;
        Ok(guard.1.get(tx_hash).cloned())
    }

    /// List donations in `(created_at, id)` order, one page at a time (#982).
    ///
    /// `cursor` is the opaque value from a previous [`PageInfo::next_cursor`];
    /// pass `None` to start at the beginning. The result is stable under
    /// concurrent inserts: new rows have a later `(created_at, id)` and so land
    /// strictly after the current page, never shifting it.
    pub fn list_donations(
        &self,
        filter: &DonationFilter,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<DonationPage, DbError> {
        let after = match cursor {
            Some(c) => Some(decode_cursor(c)?),
            None => None,
        };
        let limit = clamp_page_size(limit);

        let guard = self
            .inner
            .lock()
            .map_err(|e| DbError::Storage(e.to_string()))?;

        // The map has no order, so materialise and sort by the stable key.
        let mut matching: Vec<Donation> = guard
            .1
            .values()
            .filter(|d| filter.matches(d))
            .cloned()
            .collect();
        matching.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });

        // Resume strictly after the cursor's key within the filtered, ordered
        // set. A cursor whose boundary is no longer present still compares
        // correctly, so pruning the boundary row does not break the walk.
        let start = match &after {
            Some(key) => matching
                .iter()
                .position(|d| key_of(d) > *key)
                .unwrap_or(matching.len()),
            None => 0,
        };

        let total = matching.len() as u32;
        let end = start.saturating_add(limit as usize).min(matching.len());
        let page_items: Vec<Donation> = matching[start..end].to_vec();

        let has_more = end < matching.len();
        let next_cursor = if has_more {
            page_items.last().map(|d| encode_cursor(&d.created_at, d.id))
        } else {
            None
        };

        Ok(DonationPage {
            donations: page_items,
            page: PageInfo {
                total,
                limit,
                count: (end - start) as u32,
                has_more,
                next_cursor,
            },
        })
    }
}

impl Default for DonationsRepo {
    fn default() -> Self {
        Self::new()
    }
}

impl DonationFilter {
    fn matches(&self, donation: &Donation) -> bool {
        if let Some(status) = &self.status {
            if &donation.status != status {
                return false;
            }
        }
        if let Some(campaign_id) = &self.campaign_id {
            if &donation.campaign_id != campaign_id {
                return false;
            }
        }
        true
    }
}

/// `limit == 0` reads as "caller did not care" (the default page); anything
/// above [`MAX_PAGE_SIZE`] is capped. Mirrors the contract-side `clamp_limit`.
fn clamp_page_size(limit: u32) -> u32 {
    if limit == 0 {
        DEFAULT_PAGE_SIZE
    } else {
        limit.min(MAX_PAGE_SIZE)
    }
}

/// The `(created_at, id)` sort key, ordered lexicographically as written.
fn key_of(d: &Donation) -> (String, u64) {
    (d.created_at.clone(), d.id)
}

fn encode_cursor(created_at: &str, id: u64) -> String {
    let mut raw = String::with_capacity(created_at.len() + 20);
    raw.push_str(created_at);
    raw.push('\u{1f}');
    raw.push_str(&id.to_string());
    hex_encode(&raw)
}

fn decode_cursor(cursor: &str) -> Result<(String, u64), DbError> {
    let raw = hex_decode(cursor).ok_or_else(|| DbError::Storage("invalid cursor".to_string()))?;
    let (created_at, id) = raw
        .split_once('\u{1f}')
        .ok_or_else(|| DbError::Storage("invalid cursor".to_string()))?;
    let id = id
        .parse::<u64>()
        .map_err(|_| DbError::Storage("invalid cursor".to_string()))?;
    Ok((created_at.to_string(), id))
}

fn hex_encode(s: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.as_bytes() {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn hex_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    if bytes.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let hi = hex_val(bytes[i])?;
        let lo = hex_val(bytes[i + 1])?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    String::from_utf8(out).ok()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_donation(hash: &str) -> NewDonation {
        NewDonation {
            tx_hash: hash.to_string(),
            campaign_id: "camp-1".to_string(),
            donor_address: "GABC".to_string(),
            amount: 100,
            status: DonationStatus::Pending,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn donation_at(hash: &str, campaign: &str, created_at: &str, status: DonationStatus) -> NewDonation {
        NewDonation {
            tx_hash: hash.to_string(),
            campaign_id: campaign.to_string(),
            donor_address: "GABC".to_string(),
            amount: 100,
            status,
            created_at: created_at.to_string(),
        }
    }

    #[test]
    fn save_and_retrieve() {
        let repo = DonationsRepo::new();
        let d = repo.save_donation(&new_donation("txhash1")).unwrap();
        assert_eq!(d.tx_hash, "txhash1");
        assert_eq!(d.id, 1);
    }

    #[test]
    fn idempotent_on_duplicate_tx_hash() {
        let repo = DonationsRepo::new();
        let d1 = repo.save_donation(&new_donation("txhash2")).unwrap();
        let d2 = repo.save_donation(&new_donation("txhash2")).unwrap();
        assert_eq!(d1.id, d2.id);
    }

    #[test]
    fn empty_repo_returns_an_empty_final_page() {
        let repo = DonationsRepo::new();
        let page = repo.list_donations(&DonationFilter::default(), None, 10).unwrap();
        assert!(page.donations.is_empty());
        assert_eq!(page.page.total, 0);
        assert!(!page.page.has_more);
        assert_eq!(page.page.next_cursor, None);
    }

    #[test]
    fn pages_walk_the_whole_set_without_gaps_or_repeats() {
        let repo = DonationsRepo::new();
        for i in 0..137 {
            repo.save_donation(&donation_at(
                &format!("tx{i}"),
                "camp-1",
                &format!("2026-01-01T00:{:02}:00Z", i),
                DonationStatus::Pending,
            ))
            .unwrap();
        }

        let mut seen: Vec<u64> = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let page = repo
                .list_donations(&DonationFilter::default(), cursor.as_deref(), 25)
                .unwrap();
            assert!(page.page.count <= MAX_PAGE_SIZE);
            for d in &page.donations {
                seen.push(d.id);
            }
            match page.page.next_cursor {
                Some(next) => {
                    assert!(page.page.has_more);
                    cursor = Some(next);
                }
                None => {
                    assert!(!page.page.has_more);
                    break;
                }
            }
        }
        assert_eq!(seen.len(), 137);
        // Every id appears exactly once.
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 137);
    }

    #[test]
    fn cursor_is_stable_when_new_rows_are_inserted_between_pages() {
        // The property the issue is really about (#982): a donation confirmed
        // mid-walk must not shift the next page.
        let repo = DonationsRepo::new();
        for i in 0..10 {
            repo.save_donation(&donation_at(
                &format!("tx{i}"),
                "camp-1",
                &format!("2026-01-01T00:{:02}:00Z", i),
                DonationStatus::Pending,
            ))
            .unwrap();
        }

        let first = repo
            .list_donations(&DonationFilter::default(), None, 3)
            .unwrap();
        assert_eq!(first.donations.len(), 3);
        assert!(first.page.has_more);
        let cursor = first.page.next_cursor.clone().unwrap();

        // A new donation lands after the cursor (later timestamp and id).
        repo.save_donation(&donation_at(
            "tx-new",
            "camp-1",
            "2026-01-01T00:99:00Z",
            DonationStatus::Confirmed,
        ))
        .unwrap();

        let second = repo
            .list_donations(&DonationFilter::default(), Some(&cursor), 3)
            .unwrap();
        // The cursor still points at the same boundary: the second page begins
        // immediately after the first, with no repeat of a page-1 row.
        assert_eq!(second.donations.len(), 3);
        assert_eq!(second.donations[0].id, 4);
        assert_eq!(second.donations[1].id, 5);
        assert_eq!(second.donations[2].id, 6);
    }

    #[test]
    fn status_and_campaign_filters_compose_with_the_cursor() {
        let repo = DonationsRepo::new();
        repo.save_donation(&donation_at(
            "a",
            "camp-1",
            "2026-01-01T00:00:00Z",
            DonationStatus::Confirmed,
        ))
        .unwrap();
        repo.save_donation(&donation_at(
            "b",
            "camp-2",
            "2026-01-01T00:01:00Z",
            DonationStatus::Confirmed,
        ))
        .unwrap();
        repo.save_donation(&donation_at(
            "c",
            "camp-1",
            "2026-01-01T00:02:00Z",
            DonationStatus::Pending,
        ))
        .unwrap();
        repo.save_donation(&donation_at(
            "d",
            "camp-1",
            "2026-01-01T00:03:00Z",
            DonationStatus::Confirmed,
        ))
        .unwrap();

        let filter = DonationFilter {
            status: Some(DonationStatus::Confirmed),
            campaign_id: Some("camp-1".to_string()),
        };
        let page = repo.list_donations(&filter, None, 1).unwrap();
        assert_eq!(page.page.total, 2);
        assert_eq!(page.donations.len(), 1);
        assert_eq!(page.donations[0].tx_hash, "a");
        assert!(page.page.has_more);
        let cursor = page.page.next_cursor.clone().unwrap();

        let next = repo.list_donations(&filter, Some(&cursor), 1).unwrap();
        assert_eq!(next.donations.len(), 1);
        assert_eq!(next.donations[0].tx_hash, "d");
        assert!(!next.page.has_more);
        assert_eq!(next.page.next_cursor, None);
    }

    #[test]
    fn limit_is_clamped_to_the_page_ceiling() {
        let repo = DonationsRepo::new();
        for i in 0..(MAX_PAGE_SIZE + 20) {
            repo.save_donation(&donation_at(
                &format!("tx{i}"),
                "camp-1",
                &format!("2026-01-01T00:{:02}:00Z", i % 60),
                DonationStatus::Pending,
            ))
            .unwrap();
        }
        let page = repo
            .list_donations(&DonationFilter::default(), None, u32::MAX)
            .unwrap();
        assert_eq!(page.donations.len() as u32, MAX_PAGE_SIZE);
        assert_eq!(page.page.limit, MAX_PAGE_SIZE);
    }

    #[test]
    fn zero_limit_serves_the_default_page() {
        let repo = DonationsRepo::new();
        for i in 0..(DEFAULT_PAGE_SIZE + 10) {
            repo.save_donation(&donation_at(
                &format!("tx{i}"),
                "camp-1",
                &format!("2026-01-01T00:{:02}:00Z", i % 60),
                DonationStatus::Pending,
            ))
            .unwrap();
        }
        let page = repo.list_donations(&DonationFilter::default(), None, 0).unwrap();
        assert_eq!(page.donations.len() as u32, DEFAULT_PAGE_SIZE);
        assert_eq!(page.page.limit, DEFAULT_PAGE_SIZE);
    }

    #[test]
    fn a_malformed_cursor_is_rejected() {
        let repo = DonationsRepo::new();
        assert!(repo
            .list_donations(&DonationFilter::default(), Some("not-hex!"), 10)
            .is_err());
    }

    #[test]
    fn cursor_round_trips_through_encoding() {
        let cursor = encode_cursor("2026-01-01T00:00:00Z", 42);
        assert_eq!(
            decode_cursor(&cursor).unwrap(),
            ("2026-01-01T00:00:00Z".to_string(), 42)
        );
    }
}
