//! Invoke any deployed contract function from the command line (issue #867).
//!
//! Usage:
//!   cargo run --bin invoke_function -- \
//!     G<SOURCE> C<CONTRACT_ID> balanceOf address:G<OWNER>
//!
//! Arguments are typed: `address:G...` | `u64:5` | `i128:1000` |
//! `symbol:name` | `string:hello`.
//!
//! This is the generic building block behind every StellarAid integration:
//! encode a host-function call, simulate it, and read the cost + result.

use sdk::errors::{Result, StellarAidError};
use soroban_sdk::xdr::ScVal;

use stellar_aid_examples::{
    invoke_host_function, network_from_env, sc_val_address, sc_val_i128, sc_val_string,
    sc_val_symbol, sc_val_u64, simulate_invocation,
};

fn parse_arg(raw: &str) -> Result<ScVal> {
    let (kind, value) = raw
        .split_once(':')
        .ok_or_else(|| StellarAidError::validation(format!("arg `{raw}` must be <type>:<value>")))?;
    match kind {
        "address" => Ok(sc_val_address(value)),
        "u64" => Ok(sc_val_u64(
            value
                .parse()
                .map_err(|_| StellarAidError::validation(format!("bad u64 `{value}`")))?,
        )),
        "i128" => Ok(sc_val_i128(
            value
                .parse()
                .map_err(|_| StellarAidError::validation(format!("bad i128 `{value}`")))?,
        )),
        "symbol" => Ok(sc_val_symbol(value)),
        "string" => Ok(sc_val_string(value)),
        _ => Err(StellarAidError::validation(format!(
            "unknown arg type `{kind}` (expected address|u64|i128|symbol|string)"
        ))),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let source = args
        .next()
        .ok_or_else(|| StellarAidError::validation("usage: invoke_function SOURCE CONTRACT_ID FUNCTION [args...]"))?;
    let contract_id = args
        .next()
        .ok_or_else(|| StellarAidError::validation("usage: invoke_function SOURCE CONTRACT_ID FUNCTION [args...]"))?;
    let function = args
        .next()
        .ok_or_else(|| StellarAidError::validation("usage: invoke_function SOURCE CONTRACT_ID FUNCTION [args...]"))?;
    let call_args = args.map(|a| parse_arg(&a)).collect::<Result<Vec<_>>>()?;

    let network = network_from_env();
    let invocation = simulate_invocation(
        &network,
        &source,
        invoke_host_function(&contract_id, &function, call_args)?,
    )
    .await?;

    println!("{function}({})  contract={contract_id}", args_args(&call_args));
    println!("estimated cost: {:?}", invocation.simulation.cost);
    println!("results:        {:?}", invocation.simulation.results);
    match &invocation.simulation.error {
        None => println!("simulation OK, envelope: {}", invocation.envelope_with_fee),
        Some(err) => println!("simulation error: {err}"),
    }
    Ok(())
}

fn args_args(args: &[ScVal]) -> String {
    args.iter()
        .map(|a| format!("{a:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}