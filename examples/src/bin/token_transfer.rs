//! Build a token `transfer` and `balanceOf` call with proper ScVal encoding
//! (issue #867).
//!
//! Usage:
//!   cargo run --bin token_transfer -- \
//!     G<SOURCE> C<TOKEN_CONTRACT_ID> G<FROM> 1000000000
//!
//! Demonstrates how StellarAid encodes i128 amounts and `Address` arguments
//! for standard Soroban tokens (Stellar Asset Contract), which is the shape
//! every donation/escrow payout reuses.

use sdk::errors::{Result, StellarAidError};

use stellar_aid_examples::{
    invoke_host_function, network_from_env, sc_val_address, sc_val_i128, simulate_invocation,
};

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let source = args.next().ok_or_else(|| StellarAidError::validation("usage: token_transfer SOURCE TOKEN_CONTRACT_ID FROM AMOUNT"))?;
    let token = args.next().ok_or_else(|| StellarAidError::validation("usage: token_transfer SOURCE TOKEN_CONTRACT_ID FROM AMOUNT"))?;
    let from = args.next().ok_or_else(|| StellarAidError::validation("usage: token_transfer SOURCE TOKEN_CONTRACT_ID FROM AMOUNT"))?;
    let amount: i128 = args
        .next()
        .and_then(|a| a.parse().ok())
        .ok_or_else(|| StellarAidError::validation("amount must be an i128"))?;

    let to = std::env::var("TO_ADDRESS")
        .unwrap_or_else(|_| from.clone())
        .to_string();

    let network = network_from_env();

    let transfer = simulate_invocation(
        &network,
        &source,
        invoke_host_function(
            &token,
            "transfer",
            vec![sc_val_address(&from), sc_val_address(&to), sc_val_i128(amount)],
        )?,
    )
    .await?;
    println!("transfer simulation: {:?}", transfer.simulation.results);
    println!("estimated cost:       {:?}", transfer.simulation.cost);

    let balance = simulate_invocation(
        &network,
        &source,
        invoke_host_function(
            &token,
            "balanceOf",
            vec![sc_val_address(&from)],
        )?,
    )
    .await?;
    println!("balanceOf simulation: {:?}", balance.simulation.results);
    println!("estimated cost:       {:?}", balance.simulation.cost);
    Ok(())
}