//! Read-only liveness checks against the Soroban RPC / Horizon endpoints
//! (issue #867).
//!
//! Shows the SDK's read paths that need no signing: RPC health, account
//! sequencing, and the latest Horizon transaction for an account.

use sdk::horizon::client::HorizonClient;
use stellar_aid_examples::network_from_env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let network = network_from_env();

    let rpc = sdk::soroban::rpc_client::SorobanRpcClient::new(&network.rpc_url);
    println!("rpc health: {:?}", rpc.get_health().await);

    let horizon = HorizonClient::new(&network.horizon_url);
    match std::env::var("ACCOUNT_ADDRESS") {
        Ok(address) => {
            let account = horizon.get_account(&address).await?;
            println!("account {address}");
            println!("  sequence: {}", account.sequence);
            let recent = horizon.get_transactions(&address).await;
            println!("  recent transactions: {recent:?}");
        }
        Err(_) => println!("set ACCOUNT_ADDRESS to also inspect an account"),
    }
    Ok(())
}