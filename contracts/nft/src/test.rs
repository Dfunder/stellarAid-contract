extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env, String,
};

fn setup<'a>() -> (Env, NftOwnershipClient<'a>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_sequence_number(100);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let royalty_token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let contract_id = env.register_contract(None, NftOwnership);
    let client = NftOwnershipClient::new(&env, &contract_id);
    client.initialize(&admin, &royalty_token);

    (env, client, admin, royalty_token)
}

fn str(env: &Env, s: &str) -> String {
    String::from_str(env, s)
}

#[test]
fn test_initialize_sets_admin_and_counter() {
    let (env, client, _admin, _token) = setup();
    // NftCount was initialised to 0; the first mint must return 1.
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let id = client.mint(
        &creator,
        &admin,
        &str(&env, "Piece"),
        &str(&env, "ipfs://cid"),
        &0,
        &0,
        &true,
        &true,
    );
    assert_eq!(id, 1);
}

#[test]
fn test_mint_assigns_sequential_ids_and_stores_record() {
    let (env, client, _admin, _token) = setup();
    let creator = Address::generate(&env);
    let owner = Address::generate(&env);

    let first = client.mint(
        &creator,
        &owner,
        &str(&env, "One"),
        &str(&env, "ipfs://one"),
        &500,
        &10,
        &true,
        &true,
    );
    let second = client.mint(
        &creator,
        &owner,
        &str(&env, "Two"),
        &str(&env, "ipfs://two"),
        &0,
        &0,
        &false,
        &false,
    );

    assert_eq!(first, 1);
    assert_eq!(second, 2);

    let nft = client.get_nft(&first);
    assert_eq!(nft.id, first);
    assert_eq!(nft.creator, creator);
    assert_eq!(nft.owner, owner);
    assert_eq!(nft.royalty_bps, 500);
    assert_eq!(nft.commission_id, 10);
    assert!(nft.transferable);
    assert!(nft.burnable);
    assert_eq!(nft.status, NftStatus::Active);
}

#[test]
fn test_transfer_moves_ownership_and_records_history() {
    let (env, client, _admin, _token) = setup();
    let creator = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);

    let id = client.mint(
        &creator,
        &seller,
        &str(&env, "Piece"),
        &str(&env, "ipfs://cid"),
        &0, // no royalty -> simple transfer
        &0,
        &true,
        &true,
    );

    // price = 0 is a gift; no royalty settlement occurs.
    client.transfer(&seller, &buyer, &id, &0);

    let nft = client.get_nft(&id);
    assert_eq!(nft.owner, buyer);
    assert_eq!(client.get_transfer_count(&id), 1);

    let history = client.get_transfer_history(&id, &0, &10);
    assert_eq!(history.len(), 1);
    assert_eq!(history.get(0).unwrap().from, seller);
    assert_eq!(history.get(0).unwrap().to, buyer);
}

#[test]
fn test_transfer_settles_royalty_on_secondary_sale() {
    let (env, client, _admin, token_id) = setup();
    let creator = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);

    let id = client.mint(
        &creator,
        &seller,
        &str(&env, "Piece"),
        &str(&env, "ipfs://cid"),
        &1000, // 10% royalty
        &0,
        &true,
        &true,
    );

    // Fund the buyer so the royalty transfer can succeed.
    let token_client = token::Client::new(&env, &token_id);
    let token_admin = token::StellarAssetClient::new(&env, &token_id);
    token_admin.mint(&buyer, &1_000_000);

    let price = 100_000i128;
    client.transfer(&buyer, &seller, &id, &price);

    // Royalty = price * 1000 / 10_000 = 10_000, paid to the creator.
    assert_eq!(token_client.balance(&creator), 10_000);
    assert_eq!(client.get_nft(&id).owner, seller);
}

#[test]
fn test_burn_marks_certificate_as_burned() {
    let (env, client, _admin, _token) = setup();
    let creator = Address::generate(&env);
    let owner = Address::generate(&env);

    let id = client.mint(
        &creator,
        &owner,
        &str(&env, "Piece"),
        &str(&env, "ipfs://cid"),
        &0,
        &0,
        &true,
        &true,
    );

    client.burn(&owner, &id);
    assert_eq!(client.get_nft(&id).status, NftStatus::Burned);
}

#[test]
fn test_freeze_disables_transfer() {
    let (env, client, admin, _token) = setup();
    let creator = Address::generate(&env);
    let owner = Address::generate(&env);

    let id = client.mint(
        &creator,
        &owner,
        &str(&env, "Piece"),
        &str(&env, "ipfs://cid"),
        &0,
        &0,
        &true,
        &true,
    );
    assert!(client.get_nft(&id).transferable);

    // Admin freeze marks the certificate non-transferable (IP dispute).
    client.freeze(&admin, &id);
    assert!(!client.get_nft(&id).transferable);
}

#[test]
fn test_update_royalty_updates_bps() {
    let (env, client, _admin, _token) = setup();
    let creator = Address::generate(&env);
    let owner = Address::generate(&env);

    let id = client.mint(
        &creator,
        &owner,
        &str(&env, "Piece"),
        &str(&env, "ipfs://cid"),
        &500,
        &0,
        &true,
        &true,
    );

    client.update_royalty(&creator, &id, &1500);
    assert_eq!(client.get_nft(&id).royalty_bps, 1500);
}

#[test]
#[should_panic(expected = "royalty_bps must not exceed 3000 (30%)")]
fn test_mint_rejects_royalty_above_cap() {
    let (env, client, _admin, _token) = setup();
    let creator = Address::generate(&env);
    let owner = Address::generate(&env);
    client.mint(
        &creator,
        &owner,
        &str(&env, "Piece"),
        &str(&env, "ipfs://cid"),
        &3001,
        &0,
        &true,
        &true,
    );
}

