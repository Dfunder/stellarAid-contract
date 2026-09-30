//! Deploy a compiled StellarAid contract WASM to testnet (issue #867).
//!
//! Usage:
//!   STELLAR=G... DEPLOYER_ADDRESS=G... \
//!     cargo run --bin deploy_contract -- contracts/escrow/target/wasm32-unknown-unknown/release/escrow.wasm
//!
//! The example builds the `CreateContract` host function, simulates it against
//! the network, and prints the resulting contract-id preimage + resource cost.
//! The simulation validates that the WASM is accepted before anything is
//! submitted; signing + submission is left to the wallet layer.

use sdk::errors::StellarAidError;

use stellar_aid_examples::{create_contract_host_function, network_from_env, simulate_invocation};

#[tokio::main]
async fn main() -> Result<(), StellarAidError> {
    let wasm_path = std::env::args().nth(1).ok_or_else(|| {
        StellarAidError::validation("usage: deploy_contract <path/to/contract.wasm>")
    })?;
    let deployer = std::env::var("DEPLOYER_ADDRESS")
        .map_err(|_| StellarAidError::validation("set DEPLOYER_ADDRESS (a G... account)"))?;

    let wasm = std::fs::read(&wasm_path)
        .map_err(|e| StellarAidError::validation(format!("cannot read {wasm_path}: {e}")))?;
    let network = network_from_env();

    // A deterministic salt is fine for a demo; rotate it per real deployment.
    let invocation = simulate_invocation(
        &network,
        &deployer,
        create_contract_host_function(&wasm, &deployer, [0u8; 32]),
    )
    .await?;

    println!("deploying {} bytes of WASM from {deployer}", wasm.len());
    match &invocation.simulation.cost {
        Some(cost) => println!("estimated cost: {cost}"),
        None => println!("no cost estimate returned"),
    }
    match &invocation.simulation.error {
        Some(err) => {
            println!("simulation rejected the deployment: {err}");
            return Err(StellarAidError::contract(err.to_string()));
        }
        None => println!("simulation OK, envelope ready: {}", invocation.envelope_with_fee),
    }
    Ok(())
}