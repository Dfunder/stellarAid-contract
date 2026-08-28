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
use types::{ArtistListing, DataKey, SearchAnalytics, SearchFilters, SearchResultPage, SortBy};

include!("../../semver_types.rs");

/// Maximum number of skill tags a single listing may carry.
const MAX_SKILLS: u32 = 20;
/// Maximum number of keyword tags a single listing may carry.
const MAX_KEYWORDS: u32 = 30;
/// Hard cap on a single page of search results, bounding per-call cost.
const MAX_PAGE_SIZE: u32 = 50;

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

fn matches_filters(listing: &ArtistListing, filters: &SearchFilters) -> bool {
    if !listing.active {
        return false;
    }
    if let Some(skill) = &filters.skill {
        if !contains_string(&listing.skills, skill) {
            return false;
        }
    }
    if let Some(min_price) = filters.min_price {
        if listing.price < min_price {
            return false;
        }
    }
    if let Some(max_price) = filters.max_price {
        if listing.price > max_price {
            return false;
        }
    }
    if let Some(min_rating) = filters.min_rating {
        if listing.rating < min_rating {
            return false;
        }
    }
    if let Some(keyword) = &filters.keyword {
        if !contains_string(&listing.keywords, keyword) {
            return false;
        }
    }
    true
}

fn is_ordered(a: &ArtistListing, b: &ArtistListing, sort_by: SortBy) -> bool {
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
fn sort_listings(mut listings: Vec<ArtistListing>, sort_by: SortBy) -> Vec<ArtistListing> {
    let n = listings.len();
    let mut i = 1;
    while i < n {
        let key = listings.get(i).unwrap();
        let mut j = i;
        while j > 0 {
            let prev = listings.get(j - 1).unwrap();
            if is_ordered(&prev, &key, sort_by) {
                break;
            }
            listings.set(j, prev);
            j -= 1;
        }
        listings.set(j, key);
        i += 1;
    }
    listings
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

        if is_new {
            let mut all: Vec<Address> = env
                .storage()
                .instance()
                .get(&DataKey::AllArtists)
                .unwrap_or_else(|| Vec::new(&env));
            all.push_back(artist.clone());
            env.storage().instance().set(&DataKey::AllArtists, &all);

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

        let all: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AllArtists)
            .unwrap_or_else(|| Vec::new(&env));

        let mut matched: Vec<ArtistListing> = Vec::new(&env);
        for artist in all.iter() {
            let maybe_listing: Option<ArtistListing> =
                env.storage().persistent().get(&DataKey::Listing(artist));
            if let Some(listing) = maybe_listing {
                if matches_filters(&listing, &filters) {
                    matched.push_back(listing);
                }
            }
        }

        let total_matches = matched.len();
        let sorted = sort_listings(matched, sort_by);

        let start = page.saturating_mul(page_size);
        let mut page_results: Vec<ArtistListing> = Vec::new(&env);
        if start < total_matches {
            let end = core::cmp::min(start.saturating_add(page_size), total_matches);
            let mut i = start;
            while i < end {
                page_results.push_back(sorted.get(i).unwrap());
                i += 1;
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
        })
    }

    pub fn get_analytics(env: Env) -> SearchAnalytics {
        load_analytics(&env)
    }

    pub fn get_indexed_count(env: Env) -> u32 {
        load_analytics(&env).total_indexed
    }
}
