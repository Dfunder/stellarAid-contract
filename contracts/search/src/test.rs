#![cfg(test)]

extern crate std;

use soroban_sdk::{testutils::Address as _, Address, Bytes, Env};

use crate::{
    errors::SearchError,
    types::{SearchFilter, SortOrder},
    SearchContract, SearchContractClient,
};

fn make_env() -> Env {
    Env::default()
}

fn setup(env: &Env) -> (Address, SearchContractClient) {
    let cid = env.register_contract(None, SearchContract);
    let client = SearchContractClient::new(env, &cid);
    let admin = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin).unwrap();
    (admin, client)
}

fn b(env: &Env, s: &str) -> Bytes {
    Bytes::from_slice(env, s.as_bytes())
}

fn no_filter(env: &Env) -> SearchFilter {
    SearchFilter {
        skills: b(env, ""),
        min_price: 0,
        max_price: 0,
        min_reputation: 0,
    }
}

// ---------------------------------------------------------------------------
// Init
// ---------------------------------------------------------------------------

#[test]
fn test_double_init_fails() {
    let env = make_env();
    let cid = env.register_contract(None, SearchContract);
    let client = SearchContractClient::new(&env, &cid);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin).unwrap();
    let err = client.initialize(&admin).unwrap_err();
    assert_eq!(err, SearchError::AlreadyInitialized);
}

// ---------------------------------------------------------------------------
// Index / update / remove
// ---------------------------------------------------------------------------

#[test]
fn test_index_and_get_profile() {
    let env = make_env();
    let (_, client) = setup(&env);
    let artist = Address::generate(&env);
    env.mock_all_auths();

    client
        .index_profile(
            &artist,
            &b(&env, "illustration,design"),
            &5000i128,
            &8000u32,
            &b(&env, "top illustrator"),
        )
        .unwrap();

    let profile = client.get_profile(&artist).unwrap();
    assert_eq!(profile.min_price, 5000);
    assert_eq!(profile.reputation_score, 8000);
}

#[test]
fn test_double_index_fails() {
    let env = make_env();
    let (_, client) = setup(&env);
    let artist = Address::generate(&env);
    env.mock_all_auths();

    client
        .index_profile(&artist, &b(&env, "design"), &1000i128, &5000u32, &b(&env, ""))
        .unwrap();
    let err = client
        .index_profile(&artist, &b(&env, "design"), &1000i128, &5000u32, &b(&env, ""))
        .unwrap_err();
    assert_eq!(err, SearchError::AlreadyIndexed);
}

#[test]
fn test_remove_profile() {
    let env = make_env();
    let (_, client) = setup(&env);
    let artist = Address::generate(&env);
    env.mock_all_auths();

    client
        .index_profile(&artist, &b(&env, "art"), &2000i128, &7000u32, &b(&env, ""))
        .unwrap();
    client.remove_profile(&artist).unwrap();

    let err = client.get_profile(&artist).unwrap_err();
    assert_eq!(err, SearchError::NotFound);
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

#[test]
fn test_search_no_filter() {
    let env = make_env();
    let (_, client) = setup(&env);
    env.mock_all_auths();

    let a1 = Address::generate(&env);
    let a2 = Address::generate(&env);

    client
        .index_profile(&a1, &b(&env, "design"), &1000i128, &6000u32, &b(&env, ""))
        .unwrap();
    client
        .index_profile(&a2, &b(&env, "photo"), &2000i128, &7000u32, &b(&env, ""))
        .unwrap();

    let page = client
        .search(&no_filter(&env), &SortOrder::ReputationDesc, &0u32, &10u32)
        .unwrap();

    assert_eq!(page.results.len(), 2);
    assert_eq!(page.total_scanned, 2);
    // First result should be highest reputation (a2 = 7000)
    assert_eq!(page.results.get(0).unwrap().artist, a2);
}

#[test]
fn test_search_skill_filter() {
    let env = make_env();
    let (_, client) = setup(&env);
    env.mock_all_auths();

    let a1 = Address::generate(&env);
    let a2 = Address::generate(&env);

    client
        .index_profile(&a1, &b(&env, "design,animation"), &1000i128, &5000u32, &b(&env, ""))
        .unwrap();
    client
        .index_profile(&a2, &b(&env, "photography"), &2000i128, &6000u32, &b(&env, ""))
        .unwrap();

    let filter = SearchFilter {
        skills: b(&env, "animation"),
        min_price: 0,
        max_price: 0,
        min_reputation: 0,
    };

    let page = client
        .search(&filter, &SortOrder::ReputationDesc, &0u32, &10u32)
        .unwrap();

    assert_eq!(page.results.len(), 1);
    assert_eq!(page.results.get(0).unwrap().artist, a1);
}

#[test]
fn test_search_price_filter() {
    let env = make_env();
    let (_, client) = setup(&env);
    env.mock_all_auths();

    let a1 = Address::generate(&env);
    let a2 = Address::generate(&env);
    let a3 = Address::generate(&env);

    client
        .index_profile(&a1, &b(&env, "art"), &500i128, &5000u32, &b(&env, ""))
        .unwrap();
    client
        .index_profile(&a2, &b(&env, "art"), &1500i128, &6000u32, &b(&env, ""))
        .unwrap();
    client
        .index_profile(&a3, &b(&env, "art"), &3000i128, &7000u32, &b(&env, ""))
        .unwrap();

    let filter = SearchFilter {
        skills: b(&env, ""),
        min_price: 1000,
        max_price: 2000,
        min_reputation: 0,
    };

    let page = client
        .search(&filter, &SortOrder::PriceAsc, &0u32, &10u32)
        .unwrap();

    assert_eq!(page.results.len(), 1);
    assert_eq!(page.results.get(0).unwrap().artist, a2);
}

#[test]
fn test_search_pagination() {
    let env = make_env();
    let (_, client) = setup(&env);
    env.mock_all_auths();

    // Index 5 artists
    for i in 0..5u8 {
        let a = Address::generate(&env);
        client
            .index_profile(&a, &b(&env, "art"), &(1000 + i as i128), &5000u32, &b(&env, ""))
            .unwrap();
    }

    let page0 = client
        .search(&no_filter(&env), &SortOrder::PriceAsc, &0u32, &3u32)
        .unwrap();
    assert_eq!(page0.results.len(), 3);
    assert_eq!(page0.page_number, 0);

    let page1 = client
        .search(&no_filter(&env), &SortOrder::PriceAsc, &1u32, &3u32)
        .unwrap();
    assert_eq!(page1.results.len(), 2);
}

#[test]
fn test_invalid_page_size() {
    let env = make_env();
    let (_, client) = setup(&env);
    env.mock_all_auths();

    let err = client
        .search(&no_filter(&env), &SortOrder::ReputationDesc, &0u32, &0u32)
        .unwrap_err();
    assert_eq!(err, SearchError::InvalidPageSize);

    let err2 = client
        .search(&no_filter(&env), &SortOrder::ReputationDesc, &0u32, &51u32)
        .unwrap_err();
    assert_eq!(err2, SearchError::InvalidPageSize);
}

#[test]
fn test_search_analytics() {
    let env = make_env();
    let (_, client) = setup(&env);
    env.mock_all_auths();

    assert_eq!(client.get_search_stats(), 0u64);
    client
        .search(&no_filter(&env), &SortOrder::NewestFirst, &0u32, &10u32)
        .unwrap();
    client
        .search(&no_filter(&env), &SortOrder::NewestFirst, &0u32, &10u32)
        .unwrap();
    assert_eq!(client.get_search_stats(), 2u64);
}
