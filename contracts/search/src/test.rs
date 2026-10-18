use super::*;
use soroban_sdk::testutils::Address as _;

fn setup(env: &Env) -> (SearchClient<'_>, Address) {
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Search);
    let client = SearchClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

fn tags(env: &Env, values: &[&str]) -> Vec<String> {
    let mut v = Vec::new(env);
    for s in values {
        v.push_back(String::from_str(env, s));
    }
    v
}

fn empty_filters(env: &Env) -> SearchFilters {
    let _ = env;
    SearchFilters {
        skill: None,
        min_price: None,
        max_price: None,
        min_rating: None,
        keyword: None,
    }
}

#[test]
fn index_and_fetch_listing() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let artist = Address::generate(&env);

    client.index_artist(
        &artist,
        &tags(&env, &["illustration", "logo-design"]),
        &500_i128,
        &tags(&env, &["anime", "portrait"]),
    );

    let listing = client.get_listing(&artist);
    assert_eq!(listing.price, 500);
    assert_eq!(listing.skills.len(), 2);
    assert!(listing.active);
    assert_eq!(client.get_indexed_count(), 1);
}

#[test]
fn negative_price_is_rejected() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let artist = Address::generate(&env);
    let result = client.try_index_artist(&artist, &Vec::new(&env), &-1_i128, &Vec::new(&env));
    assert_eq!(result, Err(Ok(SearchError::InvalidPrice)));
}

#[test]
fn search_filters_by_skill_and_price_range() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let cheap_illustrator = Address::generate(&env);
    client.index_artist(
        &cheap_illustrator,
        &tags(&env, &["illustration"]),
        &100_i128,
        &Vec::new(&env),
    );
    let pricey_illustrator = Address::generate(&env);
    client.index_artist(
        &pricey_illustrator,
        &tags(&env, &["illustration"]),
        &900_i128,
        &Vec::new(&env),
    );
    let logo_designer = Address::generate(&env);
    client.index_artist(
        &logo_designer,
        &tags(&env, &["logo-design"]),
        &200_i128,
        &Vec::new(&env),
    );

    let mut filters = empty_filters(&env);
    filters.skill = Some(String::from_str(&env, "illustration"));
    filters.max_price = Some(300_i128);

    let page = client.search(&filters, &SortBy::PriceAsc, &0, &10);
    assert_eq!(page.total_matches, 1);
    assert_eq!(page.results.len(), 1);
    assert_eq!(page.results.get(0).unwrap().artist, cheap_illustrator);
}

#[test]
fn search_sorts_by_price_ascending_and_descending() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);
    client.index_artist(&a, &Vec::new(&env), &300_i128, &Vec::new(&env));
    client.index_artist(&b, &Vec::new(&env), &100_i128, &Vec::new(&env));
    client.index_artist(&c, &Vec::new(&env), &200_i128, &Vec::new(&env));

    let filters = empty_filters(&env);
    let asc = client.search(&filters, &SortBy::PriceAsc, &0, &10);
    assert_eq!(asc.results.get(0).unwrap().price, 100);
    assert_eq!(asc.results.get(1).unwrap().price, 200);
    assert_eq!(asc.results.get(2).unwrap().price, 300);

    let desc = client.search(&filters, &SortBy::PriceDesc, &0, &10);
    assert_eq!(desc.results.get(0).unwrap().price, 300);
    assert_eq!(desc.results.get(1).unwrap().price, 200);
    assert_eq!(desc.results.get(2).unwrap().price, 100);
}

#[test]
fn search_sorts_by_rating_and_respects_admin_set_rating() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    client.index_artist(&a, &Vec::new(&env), &100_i128, &Vec::new(&env));
    client.index_artist(&b, &Vec::new(&env), &100_i128, &Vec::new(&env));
    client.set_rating(&a, &40);
    client.set_rating(&b, &90);
    let _ = admin;

    let filters = empty_filters(&env);
    let page = client.search(&filters, &SortBy::RatingDesc, &0, &10);
    assert_eq!(page.results.get(0).unwrap().artist, b);
    assert_eq!(page.results.get(1).unwrap().artist, a);
}

#[test]
fn pagination_slices_the_sorted_result_set() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    for i in 0..5 {
        let artist = Address::generate(&env);
        client.index_artist(&artist, &Vec::new(&env), &(i as i128 * 10), &Vec::new(&env));
    }

    let filters = empty_filters(&env);
    let page0 = client.search(&filters, &SortBy::PriceAsc, &0, &2);
    assert_eq!(page0.total_matches, 5);
    assert_eq!(page0.results.len(), 2);
    assert_eq!(page0.results.get(0).unwrap().price, 0);
    assert_eq!(page0.results.get(1).unwrap().price, 10);

    let page2 = client.search(&filters, &SortBy::PriceAsc, &2, &2);
    assert_eq!(page2.results.len(), 1);
    assert_eq!(page2.results.get(0).unwrap().price, 40);

    let page_out_of_range = client.search(&filters, &SortBy::PriceAsc, &10, &2);
    assert_eq!(page_out_of_range.results.len(), 0);
    assert_eq!(page_out_of_range.total_matches, 5);
}

#[test]
fn keyword_filter_matches_metadata_tags() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let a = Address::generate(&env);
    client.index_artist(
        &a,
        &Vec::new(&env),
        &50_i128,
        &tags(&env, &["fantasy", "dragons"]),
    );
    let b = Address::generate(&env);
    client.index_artist(&b, &Vec::new(&env), &50_i128, &tags(&env, &["minimalist"]));

    let mut filters = empty_filters(&env);
    filters.keyword = Some(String::from_str(&env, "dragons"));
    let page = client.search(&filters, &SortBy::Newest, &0, &10);
    assert_eq!(page.total_matches, 1);
    assert_eq!(page.results.get(0).unwrap().artist, a);
}

#[test]
fn deactivated_listing_is_excluded_from_search() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let artist = Address::generate(&env);
    client.index_artist(&artist, &Vec::new(&env), &50_i128, &Vec::new(&env));

    client.deactivate_listing(&artist, &artist);
    let filters = empty_filters(&env);
    let page = client.search(&filters, &SortBy::Newest, &0, &10);
    assert_eq!(page.total_matches, 0);

    client.reactivate_listing(&admin, &artist);
    let page = client.search(&filters, &SortBy::Newest, &0, &10);
    assert_eq!(page.total_matches, 1);
}

#[test]
fn stranger_cannot_deactivate_someone_elses_listing() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let artist = Address::generate(&env);
    let stranger = Address::generate(&env);
    client.index_artist(&artist, &Vec::new(&env), &50_i128, &Vec::new(&env));

    let result = client.try_deactivate_listing(&stranger, &artist);
    assert_eq!(result, Err(Ok(SearchError::Unauthorized)));
}

#[test]
fn invalid_page_size_is_rejected() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let filters = empty_filters(&env);
    let result = client.try_search(&filters, &SortBy::Newest, &0, &0);
    assert_eq!(result, Err(Ok(SearchError::InvalidPageSize)));

    let result = client.try_search(&filters, &SortBy::Newest, &0, &1000);
    assert_eq!(result, Err(Ok(SearchError::InvalidPageSize)));
}

#[test]
fn inverted_price_range_is_rejected() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let mut filters = empty_filters(&env);
    filters.min_price = Some(100);
    filters.max_price = Some(50);
    let result = client.try_search(&filters, &SortBy::Newest, &0, &10);
    assert_eq!(result, Err(Ok(SearchError::InvalidPriceRange)));
}

#[test]
fn search_analytics_count_searches_and_indexed() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let artist = Address::generate(&env);
    client.index_artist(&artist, &Vec::new(&env), &10_i128, &Vec::new(&env));

    let filters = empty_filters(&env);
    client.search(&filters, &SortBy::Newest, &0, &10);
    client.search(&filters, &SortBy::Newest, &0, &10);

    let analytics = client.get_analytics();
    assert_eq!(analytics.total_indexed, 1);
    assert_eq!(analytics.total_searches, 2);
}

#[test]
fn reindexing_preserves_admin_set_rating() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let artist = Address::generate(&env);
    client.index_artist(&artist, &Vec::new(&env), &10_i128, &Vec::new(&env));
    client.set_rating(&artist, &77);

    client.index_artist(&artist, &tags(&env, &["new-skill"]), &20_i128, &Vec::new(&env));
    let listing = client.get_listing(&artist);
    assert_eq!(listing.rating, 77);
    assert_eq!(listing.price, 20);
    // Re-indexing an existing artist must not double-count total_indexed.
    assert_eq!(client.get_indexed_count(), 1);
}

// ── #987: bounded scan and index consistency ──────────────────────────────

#[test]
fn set_rating_and_toggles_update_the_search_index() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let artist = Address::generate(&env);
    client.index_artist(&artist, &Vec::new(&env), &50_i128, &Vec::new(&env));

    // A rating set after indexing must be visible to the index-backed filter.
    let mut filters = empty_filters(&env);
    filters.min_rating = Some(80);
    assert_eq!(
        client.search(&filters, &SortBy::RatingDesc, &0, &10).total_matches,
        0
    );
    client.set_rating(&artist, &90);
    assert_eq!(
        client.search(&filters, &SortBy::RatingDesc, &0, &10).total_matches,
        1
    );

    // A deactivation must be visible to the index-backed filter too.
    client.deactivate_listing(&artist, &artist);
    assert_eq!(
        client.search(&empty_filters(&env), &SortBy::Newest, &0, &10).total_matches,
        0
    );
    client.reactivate_listing(&artist, &artist);
    assert_eq!(
        client.search(&empty_filters(&env), &SortBy::Newest, &0, &10).total_matches,
        1
    );
}

#[test]
fn reindexing_rewrites_the_index_entry() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let artist = Address::generate(&env);
    client.index_artist(
        &artist,
        &tags(&env, &["illustration"]),
        &100_i128,
        &Vec::new(&env),
    );

    // Re-index with a new price and skill; the old index entry must be replaced,
    // not left stale.
    client.index_artist(
        &artist,
        &tags(&env, &["logo-design"]),
        &900_i128,
        &Vec::new(&env),
    );

    let mut filters = empty_filters(&env);
    filters.skill = Some(String::from_str(&env, "illustration"));
    assert_eq!(
        client.search(&filters, &SortBy::PriceAsc, &0, &10).total_matches,
        0
    );
    let mut filters = empty_filters(&env);
    filters.skill = Some(String::from_str(&env, "logo-design"));
    let page = client.search(&filters, &SortBy::PriceAsc, &0, &10);
    assert_eq!(page.total_matches, 1);
    assert_eq!(page.results.get(0).unwrap().price, 900);
    // The artist was not re-assigned a position.
    assert_eq!(client.get_indexed_count(), 1);
}

#[test]
fn full_scan_below_max_scan_is_not_truncated() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    for _ in 0..5 {
        let artist = Address::generate(&env);
        client.index_artist(&artist, &Vec::new(&env), &10_i128, &Vec::new(&env));
    }
    let page = client.search(&empty_filters(&env), &SortBy::PriceAsc, &0, &10);
    assert_eq!(page.total_matches, 5);
    assert_eq!(page.scanned, 5);
    assert!(!page.truncated);
}

#[test]
fn scan_beyond_max_scan_is_reported_as_truncated() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    for _ in 0..(MAX_SCAN + 5) {
        let artist = Address::generate(&env);
        client.index_artist(&artist, &Vec::new(&env), &10_i128, &Vec::new(&env));
    }
    let page = client.search(&empty_filters(&env), &SortBy::PriceAsc, &0, &10);
    assert_eq!(page.scanned, MAX_SCAN, "scan cost is bounded by MAX_SCAN");
    assert!(page.truncated, "partial results must be reported explicitly");
/// Tests for the bounded top-k selection (issue #992).
/// These verify that `select_page` (used by `search`) produces results
/// identical to the previous full-sort + window approach.
#[test]
fn top_k_selection_matches_full_sort_order_for_all_pages() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    // Index 10 artists with varying prices.
    let prices: [i128; 10] = [500, 200, 800, 100, 600, 300, 900, 400, 700, 50];
    for price in prices.iter() {
        let artist = Address::generate(&env);
        client.index_artist(&artist, &Vec::new(&env), price, &Vec::new(&env));
    }

    let filters = empty_filters(&env);

    // Collect all results page by page with page_size=3, sort by PriceAsc.
    let mut all_prices: soroban_sdk::Vec<i128> = soroban_sdk::Vec::new(&env);
    for p in 0..4u32 {
        let page = client.search(&filters, &SortBy::PriceAsc, &p, &3);
        for listing in page.results.iter() {
            all_prices.push_back(listing.price);
        }
    }

    // Expected: sorted ascending [50, 100, 200, 300, 400, 500, 600, 700, 800, 900]
    let expected: [i128; 10] = [50, 100, 200, 300, 400, 500, 600, 700, 800, 900];
    assert_eq!(all_prices.len(), 10);
    for (i, exp) in expected.iter().enumerate() {
        assert_eq!(all_prices.get(i as u32).unwrap(), *exp, "position {}", i);
    }
}

#[test]
fn top_k_total_matches_is_correct_across_pages() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    // Index 7 artists.
    for i in 0..7u32 {
        let artist = Address::generate(&env);
        client.index_artist(&artist, &Vec::new(&env), &(i as i128 * 100), &Vec::new(&env));
    }

    let filters = empty_filters(&env);

    // Every page should report total_matches = 7, regardless of which page.
    for p in 0..5u32 {
        let page = client.search(&filters, &SortBy::PriceAsc, &p, &2);
        assert_eq!(
            page.total_matches, 7,
            "page {} should report 7 total matches",
            p
        );
    }
}

#[test]
fn top_k_selection_sorts_by_rating_desc_correctly() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let ratings: [u32; 5] = [30, 10, 80, 50, 20];
    let mut artists = soroban_sdk::Vec::new(&env);
    for rating in ratings.iter() {
        let artist = Address::generate(&env);
        client.index_artist(&artist, &Vec::new(&env), &100_i128, &Vec::new(&env));
        client.set_rating(&artist, rating);
        artists.push_back(artist);
    }

    let filters = empty_filters(&env);
    let page = client.search(&filters, &SortBy::RatingDesc, &0, &5);
    assert_eq!(page.results.len(), 5);
    // Verify descending order.
    assert_eq!(page.results.get(0).unwrap().rating, 80);
    assert_eq!(page.results.get(1).unwrap().rating, 50);
    assert_eq!(page.results.get(2).unwrap().rating, 30);
    assert_eq!(page.results.get(3).unwrap().rating, 20);
    assert_eq!(page.results.get(4).unwrap().rating, 10);
}
