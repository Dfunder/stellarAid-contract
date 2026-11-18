#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, Symbol};

use crate::{CampaignFactory, CampaignFactoryClient};

#[test]
fn deploy_campaign_creates_a_campaign_instance() {
    let env = Env::default();
    env.mock_all_auths();

    let factory_id = env.register_contract(None, CampaignFactory);
    let client = CampaignFactoryClient::new(&env, &factory_id);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);

    client.initialize(&admin).unwrap();
    let params = Bytes::from_slice(&env, &[1, 2, 3, 4]);
    let deployed = client.deploy_campaign(&creator, &params).unwrap();

    assert!(!deployed.to_string().is_empty());
    let all = client.get_all_campaigns();
    assert_eq!(all.len(), 1);
    assert_eq!(all.get(0).unwrap(), deployed);

    let events = env.events().all();
    assert_eq!(events.len(), 1);
    let event = &events[0];
    let topic = event.topics().get(0).unwrap();
    assert_eq!(topic, Symbol::new(&env, "campaign_deployed"));
}

#[test]
fn initialize_rejects_second_init() {
    let env = Env::default();
    env.mock_all_auths();

    let factory_id = env.register_contract(None, CampaignFactory);
    let client = CampaignFactoryClient::new(&env, &factory_id);
    let admin = Address::generate(&env);

    client.initialize(&admin).unwrap();
    let result = client.try_initialize(&admin);
    assert!(result.is_err());
}

/// A factory with `a_count` campaigns from `a` and `b_count` from `b`.
fn factory_with_campaigns(
    env: &Env,
    a_count: u32,
    b_count: u32,
) -> (CampaignFactoryClient, Address, Address) {
    env.mock_all_auths();

    let factory_id = env.register_contract(None, CampaignFactory);
    let client = CampaignFactoryClient::new(env, &factory_id);
    client.initialize(&Address::generate(env)).unwrap();

    let a = Address::generate(env);
    let b = Address::generate(env);
    let params = Bytes::from_slice(env, &[1, 2, 3, 4]);

    for _ in 0..a_count {
        client.deploy_campaign(&a, &params).unwrap();
    }
    for _ in 0..b_count {
        client.deploy_campaign(&b, &params).unwrap();
    }

    (client, a, b)
}

#[test]
fn get_all_campaigns_page_walks_the_registry_in_pages() {
    let env = Env::default();
    let (client, _a, _b) = factory_with_campaigns(&env, 5, 2);

    let (page, info) = client.get_all_campaigns_page(&0, &3);
    assert_eq!(page.len(), 3);
    assert_eq!(info.total, 7);
    assert_eq!(info.count, 3);
    assert!(info.has_more);
    assert_eq!(info.next_start, Some(3));

    let (page, info) = client.get_all_campaigns_page(&3, &3);
    assert_eq!(page.len(), 3);
    assert!(info.has_more);
    assert_eq!(info.next_start, Some(6));

    // Short final page, not an error.
    let (page, info) = client.get_all_campaigns_page(&6, &3);
    assert_eq!(page.len(), 1);
    assert!(!info.has_more);
    assert_eq!(info.next_start, None);

    // Paging returns the registry in deployment order and covers it exactly once.
    let mut seen = 0;
    let mut start = 0u32;
    loop {
        let (page, info) = client.get_all_campaigns_page(&start, &3);
        seen += page.len() as u32;
        match info.next_start {
            Some(next) => {
                assert_eq!(next, start + 3);
                start = next;
            }
            None => break,
        }
    }
    assert_eq!(seen, 7);

    // The unbounded reader still returns the whole registry.
    assert_eq!(client.get_all_campaigns().len(), 7);
}

#[test]
fn get_all_campaigns_page_handles_the_edge_cases() {
    let env = Env::default();
    let (client, _a, _b) = factory_with_campaigns(&env, 2, 0);

    // start past the end: empty page, no next cursor.
    let (page, info) = client.get_all_campaigns_page(&99, &10);
    assert!(page.is_empty());
    assert_eq!(info.total, 2);
    assert_eq!(info.count, 0);
    assert!(!info.has_more);
    assert_eq!(info.next_start, None);

    // limit == 0 must not mean "return nothing": it yields the default page
    // size, which here covers the whole 2-entry registry.
    let (page, info) = client.get_all_campaigns_page(&0, &0);
    assert_eq!(info.limit, 2);
    assert_eq!(page.len(), 2);

    // An oversized limit is clamped rather than honoured. `PageInfo.limit` is
    // the span actually visited, so with 2 campaigns on file it is 2 either
    // way; the clamp against a registry longer than the cap is pinned by
    // `shared::pagination`'s own unit tests.
    let (page, info) = client.get_all_campaigns_page(&0, &(shared::pagination::MAX_LIMIT + 1));
    assert_eq!(info.limit, 2);
    assert_eq!(page.len(), 2);

    // start + limit must not wrap: both at u32::MAX.
    let (page, info) = client.get_all_campaigns_page(&u32::MAX, &u32::MAX);
    assert!(page.is_empty());
    assert!(!info.has_more);
}

#[test]
fn get_campaigns_by_creator_page_filters_then_pages() {
    let env = Env::default();
    let (client, a, b) = factory_with_campaigns(&env, 5, 2);

    // Only `a`'s campaigns, and the total counts matches rather than the
    // registry size.
    let (page, info) = client.get_campaigns_by_creator_page(&a, &0, &2);
    assert_eq!(page.len(), 2);
    assert_eq!(info.total, 5);
    assert!(info.has_more);
    assert_eq!(info.next_start, Some(2));

    // Short final page.
    let (page, info) = client.get_campaigns_by_creator_page(&a, &4, &2);
    assert_eq!(page.len(), 1);
    assert!(!info.has_more);
    assert_eq!(info.next_start, None);

    // `b`'s campaigns are disjoint from `a`'s.
    let (page, info) = client.get_campaigns_by_creator_page(&b, &0, &10);
    assert_eq!(page.len(), 2);
    assert_eq!(info.total, 2);
    assert!(!info.has_more);

    // A creator with no campaigns pages to an empty answer, not an error.
    let (page, info) = client.get_campaigns_by_creator_page(&Address::generate(&env), &0, &10);
    assert!(page.is_empty());
    assert_eq!(info.total, 0);
    assert!(!info.has_more);
    assert_eq!(info.next_start, None);

    // The unbounded reader keeps its existing signature and behaviour.
    assert_eq!(client.get_campaigns_by_creator(&a).len(), 5);
    assert_eq!(client.get_campaigns_by_creator(&b).len(), 2);
}
