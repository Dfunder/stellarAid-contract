//! End-to-end donation flow using the SDK's own donation transaction builder
//! (issue #867).
//!
//! Usage:
//!   cargo run --bin campaign_donation -- \
//!     G<DONOR> 42 250000000
//!
//! Uses `sdk::transaction_builder::build_donate_transaction_full`, the same
//! code path the worker calls, so the example is a faithful reference for
//! how applications drive StellarAid donations.

use std::error::Error;

use sdk::errors::StellarAidError;
use sdk::transaction_builder::DonationParams;

use stellar_aid_examples::network_from_env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let donor = args.next().ok_or_else(|| StellarAidError::validation("usage: campaign_donation DONOR CAMPAIGN_ID AMOUNT"))?;
    let campaign_id: u64 = args
        .next()
        .and_then(|a| a.parse().ok())
        .ok_or_else(|| StellarAidError::validation("campaign id must be a u64"))?;
    let amount: i128 = args
        .next()
        .and_then(|a| a.parse().ok())
        .ok_or_else(|| StellarAidError::validation("amount must be an i128"))?;
    let donation_contract_id = std::env::var("DONATION_CONTRACT_ID")
        .map_err(|_| StellarAidError::validation("set DONATION_CONTRACT_ID (a C... address)"))?;
    let memo = std::env::var("DONATION_MEMO").ok();

    let network = network_from_env();
    let params = DonationParams {
        donor,
        campaign_id,
        amount,
        token_address: std::env::var("TOKEN_ADDRESS").ok(),
        anonymous: std::env::var("ANONYMOUS").is_ok(),
        memo,
        donation_contract_id,
    };

    let envelope = sdk::transaction_builder::build_donate_transaction_full(&params, &network).await?;
    println!("donation transaction built and simulated:");
    println!("  envelope (sign + submit to complete): {envelope}");

    let rpc = sdk::soroban::rpc_client::SorobanRpcClient::new(&network.rpc_url);
    println!("  rpc health: {:?}", rpc.get_health().await);
    Ok(())
}