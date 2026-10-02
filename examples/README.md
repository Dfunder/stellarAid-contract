# StellarAid Rust SDK Examples (issue #867)

Complete, runnable Rust programs that show how to drive StellarAid contracts and
the StellarAid Rust SDK (`sdk/`) from an application. Each example is a proper
`src/bin/<name>.rs` binary in this standalone crate, so it can be run directly:

```bash
cd examples
cargo run --bin read_health
```

> This crate is intentionally **not** a workspace member — the contract
> workspace builds to WASM, while these examples are host-side programs. The
> empty `[workspace]` table keeps `cargo` happy when run from this directory.

## Examples

| Binary | Demonstrates |
|---|---|
| [`deploy_contract`](src/bin/deploy_contract.rs) | Deploying a contract — building a `CreateContract` host function, simulating the deployment, and validating the WASM is accepted. |
| [`invoke_function`](src/bin/invoke_function.rs) | Invoking any function — generic `InvokeHostFunction` construction with typed `ScVal` arguments (`address:`, `u64:`, `i128:`, `symbol:`, `string:`). |
| [`campaign_donation`](src/bin/campaign_donation.rs) | Using the SDK's own donation builder (`sdk::transaction_builder`) — the exact code path the worker uses. |
| [`token_transfer`](src/bin/token_transfer.rs) | Building Stellar Asset Contract calls (`transfer`/`balanceOf`) with correct `i128` amount and `Address` encoding. |
| [`listen_for_events`](src/bin/listen_for_events.rs) | Listening for events — polling `getTransactionStatus` to terminal state, the pattern the worker uses instead of long-lived subscriptions. |
| [`read_health`](src/bin/read_health.rs) | Read-only access — RPC health and Horizon account sequencing with no signing. |

## Requirements

* Rust stable (see `rust-toolchain.toml` at the repo root)
* Local testnet account (`G...`) seeded on Soroban testnet

## Environment

| Variable | Default |
|---|---|
| `SOROBAN_RPC_URL` | `https://soroban-testnet.stellar.org` |
| `HORIZON_URL` | `https://horizon-testnet.stellar.org` |
| `SOROBAN_NETWORK_PASSPHRASE` | `Test SDF Network ; September 2015` |
| `DEPLOYER_ADDRESS` | *(required by deploy_contract)* |
| `DONATION_CONTRACT_ID` | *(required by campaign_donation)* |
| `ACCOUNT_ADDRESS` | *(optional, read_health)* |

## Signing and submission

Soroban RPC simulations work on unsigned envelopes, so the examples are
signature-free and safe to run against a live network. To actually submit a
transaction, sign `envelope_with_fee` / the printed envelope with a wallet
(Soroban token transfers and the donation flow in the repo are signed through
the TypeScript `ContractClient` bindings in `sdk/bindings`, or the worker's
`wallet` module) and then call `sendTransaction`, at which point
`listen_for_events` shows how to track it to completion.