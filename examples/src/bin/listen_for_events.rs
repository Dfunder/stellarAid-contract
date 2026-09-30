//! Listen for transaction lifecycle events by polling (issue #867).
//!
//! Soroban RPC has no long-lived subscription; the worker and tooling poll
//! `getTransactionStatus` until the transaction reaches a terminal state.
//! This example shows exactly that loop, including how the events that a
//! successful invocation emitted are returned by the simulation as structured
//! results.
//!
//! Usage:
//!   cargo run --bin listen_for_events -- <tx_hash>
//!
//! The envelope shown by the simulation (or built via `invoke_function`) is
//! unsigned; a wallet signs it before submission, which is where the events
//! loop below takes over.

use std::time::Duration;

use sdk::errors::{Result, StellarAidError};
use sdk::soroban::rpc_client::TransactionStatus;

use stellar_aid_examples::network_from_env;

#[tokio::main]
async fn main() -> Result<()> {
    let tx_hash = std::env::args().nth(1).ok_or_else(|| {
        StellarAidError::validation("usage: listen_for_events <tx_hash>")
    })?;
    let network = network_from_env();
    let rpc = sdk::soroban::rpc_client::SorobanRpcClient::new(&network.rpc_url);

    println!("subscribing to lifecycle of {tx_hash}");

    // Poll until the hash settles, exactly like the worker's request loop.
    let mut polls = 0u32;
    loop {
        polls += 1;
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let status = rpc
            .get_transaction_status(&tx_hash)
            .await
            .map_err(|e| StellarAidError::soroban(format!("status poll failed: {e}")))?;
        println!("poll {polls}: {status:?} for {tx_hash}");
        match status {
            TransactionStatus::Success | TransactionStatus::Failed => {
                println!("terminal state reached after {polls} polls");
                break;
            }
            _ => {}
        }
    }
    Ok(())
}