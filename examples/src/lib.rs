//! Shared helpers for the StellarAid Rust SDK examples.
//!
//! Everything here uses only the **public** API of the `sdk` crate and the
//! Soroban XDR types, so each example can stay short and readable while still
//! being complete and runnable (`cargo run --bin <name> -- ...`).

use sdk::errors::{Result, StellarAidError};
use sdk::horizon::client::HorizonClient;
use sdk::soroban::rpc_client::{SimulationResult, SorobanRpcClient, TransactionStatus};
use sdk::transaction_builder::NetworkConfig;

use soroban_sdk::xdr::{
    ContractExecutable, ContractIdPreimage, ContractIdPreimageFromAddress,
    CreateContractHostFunction, Hash, HostFunction, InvokeHostFunctionOp, Memo,
    MuxedAccount, Operation, OperationBody, Preconditions, ScAddress, ScVal, ScVec,
    SequenceNumber, Symbol, TimeBounds, Transaction, TransactionEnvelope,
    TransactionExt, Uint256, VecM, WriteXdr,
};
use soroban_sdk::{Address, Env, String as SString};

/// Network configuration, overriding from environment variables the same way
/// the worker does. Defaults to Soroban testnet.
pub fn network_from_env() -> NetworkConfig {
    NetworkConfig {
        rpc_url: std::env::var("SOROBAN_RPC_URL")
            .unwrap_or_else(|_| "https://soroban-testnet.stellar.org".to_string()),
        horizon_url: std::env::var("HORIZON_URL")
            .unwrap_or_else(|_| "https://horizon-testnet.stellar.org".to_string()),
        network_passphrase: std::env::var("SOROBAN_NETWORK_PASSPHRASE")
            .unwrap_or_else(|_| "Test SDF Network ; September 2015".to_string()),
    }
}

/// A `G...` or `C...` strkey as a Soroban [`ScAddress`].
///
/// `Address::from_string` accepts both account (`G...`) and contract (`C...`)
/// strkeys and panics (with a clear message) when given anything else, which
/// is the right behaviour for an example program.
pub fn sc_address(address: &str) -> ScAddress {
    let env = Env::default();
    let parsed = Address::from_string(&SString::from_str(&env, address));
    ScAddress::try_from(parsed).expect("validated strkey converts to ScAddress")
}

/// Convert a `&str` into a Soroban contract-function-name `Symbol`.
fn function_symbol(name: &str) -> Result<Symbol> {
    name.try_into()
        .map_err(|_| StellarAidError::validation(format!("invalid function name `{name}`")))
}

// ── ScVal argument builders ────────────────────────────────────────────────

pub fn sc_val_address(address: &str) -> ScVal {
    ScVal::Address(sc_address(address))
}

pub fn sc_val_symbol(name: &str) -> ScVal {
    ScVal::Symbol(
        name.try_into()
            .expect("symbol name parses (<=32 bytes)"),
    )
}

pub fn sc_val_u64(value: u64) -> ScVal {
    ScVal::U64(value)
}

pub fn sc_val_i128(value: i128) -> ScVal {
    ScVal::I128(soroban_sdk::xdr::Int128Parts {
        lo: value as u64,
        hi: (value >> 64) as u64,
    })
}

pub fn sc_val_string(value: &str) -> ScVal {
    ScVal::String(
        value
            .try_into()
            .expect("sting fits ScString"),
    )
}

// ── Host-function builders ─────────────────────────────────────────────────

/// Build the [`HostFunction`] for an arbitrary contract invocation.
pub fn invoke_host_function(
    contract_id: &str,
    function: &str,
    args: Vec<ScVal>,
) -> Result<HostFunction> {
    Ok(HostFunction::InvokeHostFunction(
        InvokeHostFunctionOp {
            contract_id: sc_address(contract_id),
            function_name: function_symbol(function)?,
            parameters: ScVec(VecM::from(args)),
            auth: VecM::default(),
        },
    ))
}

/// Build the [`HostFunction`] that deploys a WASM blob as a new contract
/// instance under the given deployer (the salt makes the instance id
/// deterministic).
pub fn create_contract_host_function(wasm: &[u8], deployer: &str, salt: [u8; 32]) -> HostFunction {
    let wasm_hash = {
        use sha2::{Digest as _, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(wasm);
        let digest = hasher.finalize();
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&digest);
        bytes
    };
    HostFunction::CreateContract(CreateContractHostFunction {
        contract_id_preimage: ContractIdPreimage::SourceAccount(
            ContractIdPreimageFromAddress {
                address: sc_address(deployer),
                salt: Uint256(salt),
            },
        ),
        executable: ContractExecutable::Wasm(Hash(Uint256(wasm_hash))),
    })
}

// ── Transaction build + simulate ───────────────────────────────────────────

/// The result of running a host function against the network: the simulation
/// plus the same transaction re-encoded with the fee the network demanded.
pub struct SimulatedInvocation {
    pub simulation: SimulationResult,
    /// Base64 XDR envelope with a realistic fee, ready to be signed and
    /// submitted (see `send_and_listen`).
    pub envelope_with_fee: String,
    pub rpc: SorobanRpcClient,
}

/// Build a one-operation transaction from `source`, simulate it against the
/// network, and return the simulation + a fee-adjusted envelope.
///
/// Signing is deliberately left to the caller's wallet/key-store (see the
/// `sdk` wallet module and the TypeScript `ContractClient` in
/// `sdk/bindings`). The simulation works on an unsigned envelope.
pub async fn simulate_invocation(
    network: &NetworkConfig,
    source: &str,
    host_function: HostFunction,
) -> Result<SimulatedInvocation> {
    let horizon = HorizonClient::new(&network.horizon_url);
    let rpc = SorobanRpcClient::new(&network.rpc_url);

    let account = horizon
        .get_account(source)
        .await
        .map_err(|e| StellarAidError::horizon(format!("fetching account failed: {e}")))?;
    let seq: u64 = account
        .sequence
        .parse()
        .map_err(|_| StellarAidError::horizon("unexpected sequence number"))?;

    let source_account = source_source_account(source)?;

    let op = Operation {
        source_account: None,
        body: OperationBody::InvokeHostFunction(host_function),
    };
    let mut tx = build_transaction(source, seq + 1, op)?;
    let envelope = TransactionEnvelope::EnvelopeTypeTx(tx.clone());
    let xdr = envelope
        .to_xdr_base64()
        .map_err(|e| StellarAidError::validation(format!("XDR encoding failed: {e}")))?;

    let simulation = rpc
        .simulate_transaction(&xdr)
        .await
        .map_err(|e| StellarAidError::soroban(format!("simulation failed: {e}")))?;

    let sim_fee = simulation
        .cost
        .as_ref()
        .and_then(|c| c.get("minResourceFee").and_then(|v| v.as_u64()))
        .unwrap_or(100_000);

    // Re-encode with the fee the simulation demanded.
    tx.fee = sim_fee + 100;
    let envelope_with_fee = TransactionEnvelope::EnvelopeTypeTx(tx)
        .to_xdr_base64()
        .map_err(|e| StellarAidError::validation(format!("XDR encoding failed: {e}")))?;

    Ok(SimulatedInvocation {
        simulation,
        envelope_with_fee,
        rpc,
    })
}

/// Build the `MuxedAccount` source from a `G...` strkey.
fn source_source_account(source: &str) -> Result<MuxedAccount> {
    let strkey = stellar_strkey::Strkey::from_string(source)
        .map_err(|_| StellarAidError::validation("invalid source address"))?;
    match strkey {
        stellar_strkey::Strkey::PublicKeyEd25519(pk) => Ok(MuxedAccount::Ed25519(Uint256(pk.0))),
        _ => Err(StellarAidError::validation("source must be a G... account")),
    }
}

// ── Submit + listen (event-loop helper) ────────────────────────────────────

/// Send the signed envelope and poll until it reaches a terminal state.
/// This is the "listening for events" pattern: instead of blocking on the
/// RPC, poll `get_transaction_status` with backoff and react to each state.
pub async fn send_and_listen(
    rpc: &SorobanRpcClient,
    envelope: &str,
    description: &str,
    poll_interval_ms: u64,
) -> Result<TransactionStatus> {
    let send = rpc
        .send_transaction(envelope)
        .await
        .map_err(|e| StellarAidError::soroban(format!("send failed: {e}")))?;
    let tx_hash = send.hash;
    println!("{description}");
    println!("  submitted. tx hash: {tx_hash}");

    loop {
        tokio::time::sleep(std::time::Duration::from_millis(poll_interval_ms)).await;
        let status = rpc
            .get_transaction_status(&tx_hash)
            .await
            .map_err(|e| StellarAidError::soroban(format!("status poll failed: {e}")))?;
        println!(
            "  poll -> {}",
            match &status {
                TransactionStatus::Pending => "pending",
                TransactionStatus::Success => "success",
                TransactionStatus::Failed => "failed",
                TransactionStatus::NotFound => "not found",
            }
        );
        match status {
            TransactionStatus::Success => return Ok(TransactionStatus::Success),
            TransactionStatus::Failed => return Ok(TransactionStatus::Failed),
            _ => {}
        }
    }
}