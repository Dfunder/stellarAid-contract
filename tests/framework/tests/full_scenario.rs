//! End-to-end integration scenario demonstrating the framework (#663).
//!
//! Walks the full escrow lifecycle through contract boundaries: registry-based
//! dependency injection (#662), correlated events (#661), the classic release
//! path, and the atomic escrow→commission flow (#656).

use escrow::storage::CommissionStatus;
use integration_framework::assertions as a;
use integration_framework::dsl::Steps;
use integration_framework::{deploy_commission_stub, Environment, Scenario};
use platform_config::types::AddressEnvironment;
use soroban_sdk::{symbol_short, Bytes};

const AMOUNT: i128 = 10_000;
const FEE_BPS: u32 = 500;
const FEE: i128 = 500;

#[test]
fn full_escrow_lifecycle_with_registry_and_correlation() {
    let env = Environment::new();
    let admin = env.address();
    let platform_wallet = env.address();
    let client = env.address();
    let artist = env.address();

    env.mint(&client, 1_000_000);

    // ── Scenario 1: registry-backed dependency injection (#662) ────────────
    let mut scenario = Scenario::begin(&env, &admin, &platform_wallet, FEE_BPS);
    scenario.step("registry: register & resolve usdc for both namespaces");

    scenario.world().config.register_address(
        &AddressEnvironment::Production,
        &symbol_short!("usdc"),
        &env.usdc,
    );
    scenario.world().config.register_address(
        &AddressEnvironment::Test,
        &symbol_short!("usdc"),
        &env.usdc,
    );
    scenario.world().config.set_environment(&AddressEnvironment::Test);

    a::assert_address_resolves(&scenario.world().config, &symbol_short!("usdc"), &env.usdc);
    a::assert_event_emitted(
        &env,
        &scenario.world().config_addr,
        &[symbol_short!("addrreg"), symbol_short!("usdc")],
    );

    // ── Scenario 2: classic create → release with correlated events (#661) ─
    scenario.step("escrow: create + release");
    let mut steps = Steps::new(scenario.world());
    let cid = Bytes::from_slice(&env.env, b"comm-001");

    steps
        .create(&cid, &client, &artist, AMOUNT)
        .release(&cid);

    a::assert_balance(&env, &artist, AMOUNT - FEE);
    a::assert_balance(&env, &platform_wallet, FEE);
    a::assert_correlation_event_present(
        &env,
        symbol_short!("escrow"),
        symbol_short!("created"),
    );

    // ── Scenario 3: atomic escrow→commission migration (#656) ─────────────
    scenario.step("atomic: two-party commit then single-transaction migration");
    let (commission, commission_client) = deploy_commission_stub(&env, AMOUNT);

    let cid2 = Bytes::from_slice(&env.env, b"comm-002");
    let mut steps = Steps::new(scenario.world());
    steps
        .create(&cid2, &client, &artist, AMOUNT)
        .begin_atomic(&cid2)
        .confirm_atomic(&cid2, &client)
        .confirm_atomic(&cid2, &artist);

    let marker = steps.migrate_atomically(&cid2, &commission);
    assert_eq!(marker.participants, 2);

    a::assert_balance(&env, &commission, AMOUNT - FEE);
    a::assert_balance(&env, &platform_wallet, 2 * FEE);
    a::assert_escrow_status(&env, &scenario.world().escrow, &cid, CommissionStatus::Released);
    let _ = commission_client;

    // Regression guard: released escrows surface an atomic marker.
    let settled = scenario.world().escrow.get_atomic_commit(&cid2);
    assert_eq!(settled.state, escrow::AtomicCommitState::Settled);
}