//! Build unsigned Soroban `InvokeHostFunction` transaction envelopes (base64 XDR) for signing.
//!
//! For production submission you should **simulate** against RPC to obtain exact
//! `SorobanTransactionData` (resources + footprint). This builder can embed that
//! via [`BuildSorobanInvokeRequest::soroban_data_xdr`], or use a conservative default
//! footprint (contract instance key + placeholder resources) so the XDR is
//! structurally valid and signable.

use std::str::FromStr;

use serde_json::Value;
use stellar_baselib::account::{Account, AccountBehavior};
use stellar_baselib::address::{Address, AddressTrait};
use stellar_baselib::contract::{ContractBehavior, Contracts};
use stellar_baselib::soroban_data_builder::{Either, SorobanDataBuilder, SorobanDataBuilderBehavior};
use stellar_baselib::transaction::TransactionBehavior;
use stellar_baselib::transaction_builder::{TransactionBuilder, TransactionBuilderBehavior};
use stellar_baselib::xdr;
use stellar_baselib::xdr::WriteXdr;
use thiserror::Error;

/// Request to build a single `InvokeContract` host function operation inside a transaction.
#[derive(Debug, Clone)]
pub struct BuildSorobanInvokeRequest {
    /// Source account (G… public key)
    pub source_account: String,
    /// Current sequence (as string, e.g. `"123"`)
    pub sequence: String,
    /// Contract id (C… strkey)
    pub contract_id: String,
    /// Soroban symbol / method name
    pub function_name: String,
    /// Positional arguments as Soroban values (see [`json_to_sc_vals`])
    pub args: Vec<xdr::ScVal>,
    pub network_passphrase: String,
    pub timeout_seconds: i64,
    /// Base fee per operation (stroops); total fee = `base_fee_stroops * num_operations` (here: 1)
    pub base_fee_stroops: u32,
    /// Optional `SorobanTransactionData` from `simulateTransaction` (base64 XDR).
    /// When `None`, a minimal footprint + placeholder resources are applied.
    pub soroban_data_xdr: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BuildSorobanInvokeResult {
    pub xdr: String,
    pub fee_stroops: u32,
    pub operation_count: usize,
}

#[derive(Debug, Error)]
pub enum SorobanTxError {
    #[error("invalid source account: {0}")]
    InvalidSource(String),
    #[error("invalid contract id: {0}")]
    InvalidContract(String),
    #[error("function name is empty")]
    EmptyFunction,
    #[error("timeout must be non-negative")]
    InvalidTimeout,
    #[error("JSON argument error: {0}")]
    JsonArgs(String),
    #[error("transaction build failed: {0}")]
    BuildFailed(String),
}

/// Parse a JSON array of [`Value`] into Soroban [`xdr::ScVal`] arguments.
///
/// Supported shapes per element:
/// - `{"symbol":"name"}` — [`xdr::ScVal::Symbol`]
/// - `{"string":"..."}` — [`xdr::ScVal::String`]
/// - `{"i128":"..."}` — 128-bit signed integer as decimal string
/// - `{"u64": 1}` / `{"i64": -1}` / `{"u32": 1}` / `{"i32": -1}`
/// - `{"bool": true}`
/// - `{"address":"G…|C…|M…"}` — account / contract / muxed
/// - `{"bytes":[1,2,3]}` — byte array → [`xdr::ScVal::Bytes`]
/// - `"plain"` — if it looks like a strkey (`G`, `C`, `M`), parsed as address; else string
/// - `null` — [`xdr::ScVal::Void`]
pub fn json_to_sc_vals(args: &[Value]) -> Result<Vec<xdr::ScVal>, SorobanTxError> {
    args.iter().map(json_to_sc_val).collect()
}

fn json_to_sc_val(v: &Value) -> Result<xdr::ScVal, SorobanTxError> {
    match v {
        Value::Null => Ok(xdr::ScVal::Void),
        Value::Bool(b) => Ok(xdr::ScVal::Bool(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                return Ok(xdr::ScVal::I64(i));
            }
            if let Some(u) = n.as_u64() {
                return Ok(xdr::ScVal::U64(u));
            }
            Err(SorobanTxError::JsonArgs(format!("unsupported number: {}", n)))
        },
        Value::String(s) => parse_string_arg(s),
        Value::Array(a) => {
            let mut bytes = Vec::with_capacity(a.len());
            for item in a {
                let b = item
                    .as_u64()
                    .and_then(|u| u.try_into().ok())
                    .ok_or_else(|| SorobanTxError::JsonArgs("bytes array must be u8 values".into()))?;
                bytes.push(b);
            }
            let sc = xdr::ScBytes::try_from(bytes).map_err(|e| SorobanTxError::JsonArgs(e.to_string()))?;
            Ok(xdr::ScVal::Bytes(sc))
        },
        Value::Object(map) => {
            if let Some(s) = map.get("symbol").and_then(|x| x.as_str()) {
                let sym = xdr::ScSymbol::from(
                    xdr::StringM::from_str(s).map_err(|e| SorobanTxError::JsonArgs(e.to_string()))?,
                );
                return Ok(xdr::ScVal::Symbol(sym));
            }
            if let Some(s) = map.get("string").and_then(|x| x.as_str()) {
                let st = xdr::ScString::from(
                    xdr::StringM::from_str(s).map_err(|e| SorobanTxError::JsonArgs(e.to_string()))?,
                );
                return Ok(xdr::ScVal::String(st));
            }
            if let Some(s) = map.get("i128").and_then(|x| x.as_str()) {
                let n: i128 = s
                    .parse()
                    .map_err(|_| SorobanTxError::JsonArgs(format!("bad i128: {s}")))?;
                return Ok(xdr::ScVal::I128(i128_to_parts(n)));
            }
            if let Some(s) = map.get("u128").and_then(|x| x.as_str()) {
                let n: u128 = s
                    .parse()
                    .map_err(|_| SorobanTxError::JsonArgs(format!("bad u128: {s}")))?;
                return Ok(xdr::ScVal::U128(u128_to_parts(n)));
            }
            if let Some(n) = map.get("u64").and_then(|x| x.as_u64()) {
                return Ok(xdr::ScVal::U64(n));
            }
            if let Some(n) = map.get("i64").and_then(|x| x.as_i64()) {
                return Ok(xdr::ScVal::I64(n));
            }
            if let Some(n) = map.get("u32").and_then(|x| x.as_u64()) {
                return Ok(xdr::ScVal::U32(n as u32));
            }
            if let Some(n) = map.get("i32").and_then(|x| x.as_i64()) {
                return Ok(xdr::ScVal::I32(n as i32));
            }
            if let Some(b) = map.get("bool") {
                return Ok(xdr::ScVal::Bool(b.as_bool().unwrap_or(false)));
            }
            if let Some(s) = map.get("address").and_then(|x| x.as_str()) {
                let addr = Address::new(s).map_err(|e| SorobanTxError::JsonArgs(e.to_string()))?;
                return addr.to_sc_val().map_err(|e| SorobanTxError::JsonArgs(e.to_string()));
            }
            if let Some(Value::Array(raw)) = map.get("bytes") {
                return json_to_sc_val(&Value::Array(raw.clone()));
            }
            Err(SorobanTxError::JsonArgs(format!(
                "unrecognized object argument: {}",
                serde_json::to_string(v).unwrap_or_default()
            )))
        },
    }
}

fn parse_string_arg(s: &str) -> Result<xdr::ScVal, SorobanTxError> {
    let t = s.trim();
    if t.starts_with('G') || t.starts_with('C') || t.starts_with('M') {
        if let Ok(addr) = Address::new(t) {
            return addr.to_sc_val().map_err(|e| SorobanTxError::JsonArgs(e.to_string()));
        }
    }
    let st = xdr::ScString::from(
        xdr::StringM::from_str(t).map_err(|e| SorobanTxError::JsonArgs(e.to_string()))?,
    );
    Ok(xdr::ScVal::String(st))
}

fn i128_to_parts(n: i128) -> xdr::Int128Parts {
    xdr::Int128Parts {
        hi: (n >> 64) as i64,
        lo: n as u64,
    }
}

fn u128_to_parts(n: u128) -> xdr::UInt128Parts {
    xdr::UInt128Parts {
        hi: (n >> 64) as u64,
        lo: n as u64,
    }
}

fn default_soroban_data(contract: &Contracts) -> xdr::SorobanTransactionData {
    let key = contract.get_footprint();
    let mut builder = SorobanDataBuilder::new(None);
    builder.append_footprint(vec![key], vec![]);
    // Placeholder resources — replace with simulation output for real submission.
    builder.set_resources(10_000_000, 2_000_000, 1_000_000);
    builder.set_refundable_fee(20_000_000);
    builder.build()
}

/// Build an unsigned [`xdr::TransactionEnvelope`] (base64) containing one `InvokeContract` op.
pub fn build_soroban_invoke_transaction(
    request: BuildSorobanInvokeRequest,
) -> Result<BuildSorobanInvokeResult, SorobanTxError> {
    if request.timeout_seconds < 0 {
        return Err(SorobanTxError::InvalidTimeout);
    }
    let fname = request.function_name.trim();
    if fname.is_empty() {
        return Err(SorobanTxError::EmptyFunction);
    }

    let contract = Contracts::new(&request.contract_id)
        .map_err(|e: &'static str| SorobanTxError::InvalidContract(e.to_string()))?;
    let op = contract.call(fname, Some(request.args));

    let soroban_data = match &request.soroban_data_xdr {
        Some(b64) if !b64.trim().is_empty() => {
            SorobanDataBuilder::new(Some(Either::Left(b64.trim().to_string()))).build()
        },
        _ => default_soroban_data(&contract),
    };

    let mut source_account = Account::new(&request.source_account, &request.sequence)
        .map_err(SorobanTxError::InvalidSource)?;

    let mut tx_builder =
        TransactionBuilder::new(&mut source_account, &request.network_passphrase, None);
    tx_builder
        .fee(request.base_fee_stroops)
        .add_operation(op)
        .set_soroban_data(soroban_data)
        .set_timeout(request.timeout_seconds)
        .map_err(SorobanTxError::BuildFailed)?;

    let transaction = tx_builder.build();
    let op_count = transaction.operations.as_ref().map(|o| o.len()).unwrap_or(1);
    let fee_stroops = transaction.fee;

    let envelope = transaction
        .to_envelope()
        .map_err(|e| SorobanTxError::BuildFailed(e.to_string()))?;

    let xdr = envelope
        .to_xdr_base64(xdr::Limits::none())
        .map_err(|e| SorobanTxError::BuildFailed(e.to_string()))?;

    Ok(BuildSorobanInvokeResult {
        xdr,
        fee_stroops,
        operation_count: op_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_baselib::keypair::{Keypair, KeypairBehavior};
    use stellar_baselib::xdr::ReadXdr;

    const CONTRACT: &str = "CA3D5KRYM6CB7OWQ6TWYRR3Z4T7GNZLKERYNZGGA5SOAOPIFY6YQGAXE";

    #[test]
    fn builds_invoke_contract_xdr_without_args() {
        let kp = Keypair::random().unwrap();
        let req = BuildSorobanInvokeRequest {
            source_account: kp.public_key(),
            sequence: "1".to_string(),
            contract_id: CONTRACT.to_string(),
            function_name: "ping".to_string(),
            args: vec![],
            network_passphrase: "Test SDF Network ; September 2015".to_string(),
            timeout_seconds: 300,
            base_fee_stroops: 100,
            soroban_data_xdr: None,
        };
        let out = build_soroban_invoke_transaction(req).expect("build");
        assert!(!out.xdr.is_empty());
        assert_eq!(out.operation_count, 1);

        let env = xdr::TransactionEnvelope::from_xdr_base64(&out.xdr, xdr::Limits::none()).unwrap();
        match env {
            xdr::TransactionEnvelope::Tx(te) => {
                assert_eq!(te.tx.operations.len(), 1);
                assert!(matches!(te.tx.ext, xdr::TransactionExt::V1(_)));
                match &te.tx.operations[0].body {
                    xdr::OperationBody::InvokeHostFunction(ih) => {
                        match &ih.host_function {
                            xdr::HostFunction::InvokeContract(args) => {
                                assert_eq!(args.function_name.to_string(), "ping");
                                assert!(args.args.is_empty());
                            },
                            _ => panic!("expected InvokeContract"),
                        }
                    },
                    _ => panic!("expected invoke host"),
                }
            },
            _ => panic!("expected Tx envelope"),
        }
    }

    #[test]
    fn json_parses_typed_args() {
        let pk = Keypair::random().unwrap().public_key();
        let json = format!(
            r#"[{{"symbol":"init"}},{{"string":"hello"}},{{"i128":"42"}},{{"bool":true}},{{"address":"{}"}}]"#,
            pk
        );
        let parsed: Vec<Value> = serde_json::from_str(&json).unwrap();
        let vals = json_to_sc_vals(&parsed).unwrap();
        assert_eq!(vals.len(), 5);
    }
}
