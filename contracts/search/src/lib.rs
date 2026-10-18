//! Search contract — indexes artists for discovery with filtering, sorting,
//! pagination, keyword ("full-text") metadata, and search analytics (#599).
//!
//! This contract intentionally does not attempt real off-chain-search-engine
//! style full-text indexing on-chain — that is infeasible to do cheaply in a
//! Soroban contract. Instead it maintains a tag/keyword index that clients
//! populate, and scans+filters+sorts+paginates the indexed set per query.
//! Result sets are bounded by `MAX_PAGE_SIZE` and listings are bounded by
//! `MAX_SKILLS`/`MAX_KEYWORDS`, keeping each call's cost predictable.

#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, String, Vec};

pub mod errors;
pub mod types;

#[cfg(test)]
mod test;

use errors::SearchError;
use types::{ArtistIndexEntry, ArtistListing, DataKey, SearchAnalytics, SearchFilters, SearchResultPage, SortBy};

include!("../../semver_types.rs");

/// Maximum number of skill tags a single listing may carry.
const MAX_SKILLS: u32 = 20;
/// Maximum number of keyword tags a single listing may carry.
const MAX_KEYWORDS: u32 = 30;
/// Hard cap on a single page of search results, bounding per-call cost.
const MAX_PAGE_SIZE: u32 = 50;
/// Hard cap on how many index entries one `search` call examines, bounding the
/// scan cost independently of how many artists are indexed (#987).
///
/// Chosen to match the workspace's largest bounded read
/// (`shared::pagination::MAX_LIMIT`), so one search costs no more read entries
/// than the most expensive read the platform already ships. When the index has
/// more entries than this, the result is partial: `SearchResultPage::truncated`
/// is set and `scanned` reports how far the scan reached.
const MAX_SCAN: u32 = 200;

#[contract]
pub struct Search;

fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

fn get_admin(env: &Env) -> Result<Address, SearchError> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(SearchError::NotInitialized)
}

fn require_admin(env: &Env) -> Result<Address, SearchError> {
    let admin = get_admin(env)?;
    admin.require_auth();
    Ok(admin)
}

fn load_listing(env: &Env, artist: &Address) -> Result<ArtistListing, SearchError> {
    env.storage()
        .persistent()
        .get(&DataKey::Listing(artist.clone()))
        .ok_or(SearchError::ListingNotFound)
}

fn save_listing(env: &Env, listing: &ArtistListing) {
    env.storage()
        .persistent()
        .set(&DataKey::Listing(listing.artist.clone()), listing);
}

fn load_analytics(env: &Env) -> SearchAnalytics {
    env.storage()
        .instance()
        .get(&DataKey::Analytics)
        .unwrap_or_default()
}

fn save_analytics(env: &Env, analytics: &SearchAnalytics) {
    env.storage().instance().set(&DataKey::Analytics, analytics);
}

fn contains_string(haystack: &Vec<String>, needle: &String) -> bool {
    haystack.iter().any(|s| s == *needle)
}

/// Build the denormalised index entry for a listing (#987).
fn index_entry(listing: &ArtistListing) -> ArtistIndexEntry {
    ArtistIndexEntry {
        artist: listing.artist.clone(),
        skills: listing.skills.clone(),
        price: listing.price,
        rating: listing.rating,
        keywords: listing.keywords.clone(),
        indexed_ledger: listing.indexed_ledger,
        active: listing.active,
    }
}

/// Load the artist's index position, if they have been indexed.
fn artist_index(env: &Env, artist: &Address) -> Option<u32> {
    env.storage()
        .persistent()
        .get(&DataKey::ArtistIndex(artist.clone()))
}

/// Persist the denormalised entry at `index`, keeping it consistent with the
/// listing it mirrors. Every write that changes a filterable field must call
/// this (#987).
fn save_index_entry(env: &Env, index: u32, listing: &ArtistListing) {
    env.storage()
        .persistent()
        .set(&DataKey::Index(index), &index_entry(listing));
}

/// Filter using only the index entry's fields, so matching never reads the full
/// `Listing` (#987).
fn matches_filters(entry: &ArtistIndexEntry, filters: &SearchFilters) -> bool {
    if !entry.active {
        return false;
    }
    if let Some(skill) = &filters.skill {
        if !contains_string(&entry.skills, skill) {
            return false;
        }
    }
    if let Some(min_price) = filters.min_price {
        if entry.price < min_price {
            return false;
        }
    }
    if let Some(max_price) = filters.max_price {
        if entry.price > max_price {
            return false;
        }
    }
    if let Some(min_rating) = filters.min_rating {
        if entry.rating < min_rating {
            return false;
        }
    }
    if let Some(keyword) = &filters.keyword {
        if !contains_string(&entry.keywords, keyword) {
            return false;
        }
    }
    true
}

fn is_ordered(a: &ArtistIndexEntry, b: &ArtistIndexEntry, sort_by: SortBy) -> bool {
    // True when `a` should come at or before `b` under `sort_by`.
    match sort_by {
        SortBy::PriceAsc => a.price <= b.price,
        SortBy::PriceDesc => a.price >= b.price,
        SortBy::RatingDesc => a.rating >= b.rating,
        SortBy::RatingAsc => a.rating <= b.rating,
        SortBy::Newest => a.indexed_ledger >= b.indexed_ledger,
    }
}

/// Stable insertion sort over the (already filtered, so bounded) match set.
/// `soroban_sdk::Vec` gives O(1) get/set, which is all insertion sort needs.
fn sort_entries(mut entries: Vec<ArtistIndexEntry>, sort_by: SortBy) -> Vec<ArtistIndexEntry> {
    let n = entries.len();
    let mut i = 1;
    while i < n {
        let key = entries.get(i).unwrap();
        let mut j = i;
        while j > 0 {
            let prev = entries.get(j - 1).unwrap();
            if is_ordered(&prev, &key, sort_by) {
                break;
            }
            entries.set(j, prev);
            j -= 1;
        }
        entries.set(j, key);
        i += 1;
    }
    entries
}

#[contractimpl]
impl Search {
    pub fn initialize(env: Env, admin: Address) -> Result<(), SearchError> {
        if has_admin(&env) {
            return Err(SearchError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.events().publish((symbol_short!("init"),), admin);
        Ok(())
    }

    impl_semver_queries!();

    /// Create or update the caller's own search listing. Rating is not
    /// settable here — see `set_rating`.
    pub fn index_artist(
        env: Env,
        artist: Address,
        skills: Vec<String>,
        price: i128,
        keywords: Vec<String>,
    ) -> Result<(), SearchError> {
        if !has_admin(&env) {
            return Err(SearchError::NotInitialized);
        }
        artist.require_auth();

        if price < 0 {
            return Err(SearchError::InvalidPrice);
        }
        if skills.len() > MAX_SKILLS {
            return Err(SearchError::TooManySkills);
        }
        if keywords.len() > MAX_KEYWORDS {
            return Err(SearchError::TooManyKeywords);
        }

        let is_new = !env
            .storage()
            .persistent()
            .has(&DataKey::Listing(artist.clone()));
        let rating = if is_new {
            0
        } else {
            load_listing(&env, &artist)?.rating
        };

        let listing = ArtistListing {
            artist: artist.clone(),
            skills,
            price,
            rating,
            keywords,
            indexed_ledger: env.ledger().sequence(),
            active: true,
        };
        save_listing(&env, &listing);

        // #987: maintain the denormalised index entry. A new artist gets the next
        // position; an existing one keeps their position and has the entry
        // rewritten, so `search` never filters on stale values.
        let idx = match artist_index(&env, &artist) {
            Some(i) => i,
            None => {
                let i: u32 = env
                    .storage()
                    .instance()
                    .get(&DataKey::IndexCount)
                    .unwrap_or(0);
                env.storage()
                    .persistent()
                    .set(&DataKey::ArtistIndex(artist.clone()), &i);
                env.storage()
                    .instance()
                    .set(&DataKey::IndexCount, &(i + 1));
                i
            }
        };
        save_index_entry(&env, idx, &listing);

        if is_new {
            let mut analytics = load_analytics(&env);
            analytics.total_indexed += 1;
            save_analytics(&env, &analytics);
        }

        env.events()
            .publish((symbol_short!("indexed"),), (artist, price));
        Ok(())
    }

    /// Admin-set rating snapshot, typically synced from the reputation
    /// contract. Kept out of the artist's own control to prevent
    /// self-inflated search ranking.
    pub fn set_rating(env: Env, artist: Address, rating: u32) -> Result<(), SearchError> {
        require_admin(&env)?;
        if rating > 100 {
            return Err(SearchError::InvalidRating);
        }
        let mut listing = load_listing(&env, &artist)?;
        listing.rating = rating;
        save_listing(&env, &listing);
        // #987: keep the filterable index copy consistent, or `search` would rank
        // on the stale rating.
        if let Some(idx) = artist_index(&env, &artist) {
            save_index_entry(&env, idx, &listing);
        }
        env.events()
            .publish((symbol_short!("rating"),), (artist, rating));
        Ok(())
    }

    /// The artist (or admin) removes the listing from search results without
    /// losing its history; `index_artist` reactivates it.
    pub fn deactivate_listing(env: Env, caller: Address, artist: Address) -> Result<(), SearchError> {
        let admin = get_admin(&env)?;
        caller.require_auth();
        if caller != artist && caller != admin {
            return Err(SearchError::Unauthorized);
        }
        let mut listing = load_listing(&env, &artist)?;
        listing.active = false;
        save_listing(&env, &listing);
        // #987: the index carries `active`, so it must follow the toggle or a
        // deactivated artist stays discoverable.
        if let Some(idx) = artist_index(&env, &artist) {
            save_index_entry(&env, idx, &listing);
        }
        env.events()
            .publish((symbol_short!("deact"),), artist);
        Ok(())
    }

    pub fn reactivate_listing(env: Env, caller: Address, artist: Address) -> Result<(), SearchError> {
        let admin = get_admin(&env)?;
        caller.require_auth();
        if caller != artist && caller != admin {
            return Err(SearchError::Unauthorized);
        }
        let mut listing = load_listing(&env, &artist)?;
        listing.active = true;
        save_listing(&env, &listing);
        if let Some(idx) = artist_index(&env, &artist) {
            save_index_entry(&env, idx, &listing);
        }
        env.events().publish((symbol_short!("react"),), artist);
        Ok(())
    }

    pub fn get_listing(env: Env, artist: Address) -> Result<ArtistListing, SearchError> {
        load_listing(&env, &artist)
    }

    /// Filter, sort, and paginate the indexed artist set. `page` is
    /// zero-based. Also increments the search-analytics counter.
    pub fn search(
        env: Env,
        filters: SearchFilters,
        sort_by: SortBy,
        page: u32,
        page_size: u32,
    ) -> Result<SearchResultPage, SearchError> {
        if !has_admin(&env) {
            return Err(SearchError::NotInitialized);
        }
        if page_size == 0 || page_size > MAX_PAGE_SIZE {
            return Err(SearchError::InvalidPageSize);
        }
        if let (Some(min), Some(max)) = (filters.min_price, filters.max_price) {
            if min > max {
                return Err(SearchError::InvalidPriceRange);
            }
        }

        // #987: scan a bounded window of denormalised index entries. Cost is
        // capped at `MAX_SCAN` reads regardless of how many artists exist, and
        // the filter needs no per-artist `Listing` read at all.
        let count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::IndexCount)
            .unwrap_or(0);
        let scan_end = count.min(MAX_SCAN);

        let mut matched: Vec<ArtistIndexEntry> = Vec::new();
        let mut i = 0u32;
        while i < scan_end {
            if let Some(entry) = env
                .storage()
                .persistent()
                .get::<DataKey, ArtistIndexEntry>(&DataKey::Index(i))
            {
                if matches_filters(&entry, &filters) {
                    matched.push_back(entry);
                }
            }
            i += 1;
        }

        let total_matches = matched.len();
        let sorted = sort_entries(matched, sort_by);

        // Only the returned page's full listings are read from persistent
        // storage, so a page of k costs k reads, not n.
        let start = page.saturating_mul(page_size);
        let mut page_results: Vec<ArtistListing> = Vec::new();
        if start < total_matches {
            let end = core::cmp::min(start.saturating_add(page_size), total_matches);
            let mut j = start;
            while j < end {
                let entry = sorted.get(j).unwrap();
                if let Some(listing) = env
                    .storage()
                    .persistent()
                    .get::<DataKey, ArtistListing>(&DataKey::Listing(entry.artist))
                {
                    page_results.push_back(listing);
                }
                j += 1;
            }
        }

        let mut analytics = load_analytics(&env);
        analytics.total_searches += 1;
        save_analytics(&env, &analytics);

        Ok(SearchResultPage {
            results: page_results,
            total_matches,
            page,
            page_size,
            scanned: scan_end,
            truncated: count > MAX_SCAN,
        })
    }

    pub fn get_analytics(env: Env) -> SearchAnalytics {
        load_analytics(&env)
    }

    pub fn get_indexed_count(env: Env) -> u32 {
        load_analytics(&env).total_indexed
    }
}
