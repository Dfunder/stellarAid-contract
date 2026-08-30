//! In-process Soroban test environment shared by integration scenarios.
//!
//! Bundles the ingredients every multi-contract test needs: a mock-auth
//! policy, a mintable USDC asset, address generation, contract deployment,
//! and ledger helpers.

use soroban_sdk::testutils::{Address as _, ContractFunctionSet, Ledger};
use soroban_sdk::{token, Address, Env};

/// A ready-to-use test environment for integration scenarios (#663).
pub struct Environment {
    pub env: Env,
    pub usdc: Address,
    pub usdc_admin: Address,
}

impl Environment {
    pub fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        let usdc_admin = Address::generate(&env);
        let usdc = env
            .register_stellar_asset_contract_v2(usdc_admin.clone())
            .address();
        Self { env, usdc, usdc_admin }
    }

    /// Generate a fresh native account address.
    pub fn address(&self) -> Address {
        Address::generate(&self.env)
    }

    /// Deploy a contract instance and return its address.
    pub fn deploy(&self, contract: impl ContractFunctionSet + 'static) -> Address {
        self.env.register_contract(None, contract)
    }

    pub fn ledger_sequence(&self) -> u32 {
        self.env.ledger().sequence()
    }

    pub fn advance_ledgers(&self, by: u32) {
        self.env
            .ledger()
            .set_sequence_number(self.ledger_sequence() + by);
    }

    /// Mint USDC to an account.
    pub fn mint(&self, to: &Address, amount: i128) {
        token::StellarAssetClient::new(&self.env, &self.usdc).mint(to, &amount);
    }

    /// Read the USDC balance of an account.
    pub fn balance(&self, account: &Address) -> i128 {
        token::Client::new(&self.env, &self.usdc).balance(account)
    }
}

impl Default for Environment {
    fn default() -> Self {
        Self::new()
    }
}