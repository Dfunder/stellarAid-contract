//! Contract state fixtures.
//!
//! Deploys and wires the platform configuration and escrow contracts into a
//! [`World`], giving scenarios a type-safe handle to named on-chain state.

use escrow::EscrowContract;
use escrow::EscrowContractClient;
use platform_config::PlatformConfigContract;
use platform_config::PlatformConfigContractClient;
use soroban_sdk::{Address, Bytes};

use crate::Environment;

/// A deployed platform-config + escrow pair sharing one [`Environment`].
pub struct World<'a> {
    pub environment: &'a Environment,
    pub config: PlatformConfigContractClient<'a>,
    pub config_addr: Address,
    /// Legacy-interface config contract injected into escrow calls. The escrow
    /// contract consumes `get_fee_b`/`get_usdc`/`get_adm`/`get_pw`, whereas the
    /// registry-first `platform_config` exposes `resolve_for_environment`.
    pub config_stub_addr: Address,
    pub escrow: EscrowContractClient<'a>,
    pub escrow_addr: Address,
    pub admin: Address,
    pub platform_wallet: Address,
    pub fee_bps: u32,
}

impl<'a> World<'a> {
    /// Deploy `platform_config` + `escrow` and initialize the config with the
    /// given admin, fee and platform wallet.
    pub fn deploy(
        environment: &'a Environment,
        admin: &Address,
        platform_wallet: &Address,
        fee_bps: u32,
    ) -> Self {
        let env = &environment.env;

        let config_addr = env.register_contract(None, PlatformConfigContract);
        let config = PlatformConfigContractClient::new(env, &config_addr);
        config.initialize(admin, &fee_bps, platform_wallet, &environment.usdc);

        let config_stub_addr = env.register_contract(None, crate::config::ConfigStub);
        crate::config::ConfigStubClient::new(env, &config_stub_addr).init(
            &fee_bps, &environment.usdc, admin, platform_wallet,
        );

        let escrow_addr = env.register_contract(None, EscrowContract);
        let escrow = EscrowContractClient::new(env, &escrow_addr);

        Self {
            environment,
            config,
            config_addr,
            config_stub_addr,
            escrow,
            escrow_addr,
            admin: admin.clone(),
            platform_wallet: platform_wallet.clone(),
            fee_bps,
        }
    }

    /// Create a locked escrow (`#create_escrow`) for `commission_id`.
    pub fn fund_escrow(
        &self,
        commission_id: &Bytes,
        client: &Address,
        artist: &Address,
        amount: i128,
    ) {
        self.escrow
            .create_escrow(commission_id, client, artist, &amount, &self.config_stub_addr);
    }
}

/// Deploy the framework's stand-in commission agreement which answers the
/// `get_agreement_escrow_amount` probe used by the atomic flow. Returns the
/// contract address and its client.
pub fn deploy_commission_stub<'a>(
    environment: &'a Environment,
    expected_amount: i128,
) -> (Address, crate::commission::CommissionStubClient<'a>) {
    let addr = environment
        .env
        .register_contract(None, crate::commission::CommissionStub);
    let client = crate::commission::CommissionStubClient::new(&environment.env, &addr);
    client.init(&expected_amount);
    (addr, client)
}