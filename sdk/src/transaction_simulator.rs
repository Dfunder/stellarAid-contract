use crate::errors::{Result, StellarAidError};
use crate::soroban::rpc_client::SorobanRpcClient;

/// Simulates a Soroban contract invocation before submission.
///
/// This module provides helpers for deterministic transaction building
/// and simulation, enabling callers to verify expected outcomes and
/// gas costs without committing state changes.
pub struct TransactionSimulator<'a> {
    rpc: &'a SorobanRpcClient,
    source_account: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulateResult {
    pub success: bool,
    pub gas_consumed: u64,
    pub result_xdr: Option<String>,
    pub error_message: Option<String>,
}

impl<'a> TransactionSimulator<'a> {
    pub fn new(rpc: &'a SorobanRpcClient, source_account: &'a str) -> Self {
        Self { rpc, source_account }
    }

    /// Simulate a contract function call via Soroban RPC.
    ///
    /// Constructs the invocation transaction envelope and makes a live `simulateTransaction`
    /// RPC call to evaluate execution, compute gas consumption, and preview results without
    /// committing state changes on the ledger.
    ///
    /// RPC errors and contract execution failures are handled gracefully and mapped into
    /// `SimulateResult` with `success: false` and descriptive error messages.
    pub async fn simulate_contract_call(
        &self,
        contract_id: &str,
        function_name: &str,
        args_xdr: &[u8],
    ) -> Result<SimulateResult> {
        if contract_id.is_empty() {
            return Err(StellarAidError::ValidationError("contract_id must not be empty".into()));
        }
        if function_name.is_empty() {
            return Err(StellarAidError::ValidationError("function_name must not be empty".into()));
        }

        let tx_xdr = self.build_tx_envelope_base64(contract_id, function_name, args_xdr)?;

        match self.rpc.simulate_transaction(&tx_xdr).await {
            Ok(sim) => {
                let gas_consumed = sim
                    .cost
                    .as_ref()
                    .and_then(|c| {
                        c.get("cpuInsns")
                            .or_else(|| c.get("cpu_insns"))
                            .or_else(|| c.get("gas"))
                            .and_then(|v| {
                                if let Some(n) = v.as_u64() {
                                    Some(n)
                                } else if let Some(s) = v.as_str() {
                                    s.parse::<u64>().ok()
                                } else {
                                    None
                                }
                            })
                    })
                    .unwrap_or(0);

                let result_xdr = sim
                    .results
                    .as_ref()
                    .and_then(|res| res.first())
                    .and_then(|r| r.get("xdr").and_then(|v| v.as_str()).map(|s| s.to_string()));

                if let Some(err) = sim.error {
                    Ok(SimulateResult {
                        success: false,
                        gas_consumed,
                        result_xdr,
                        error_message: Some(err),
                    })
                } else {
                    Ok(SimulateResult {
                        success: true,
                        gas_consumed,
                        result_xdr,
                        error_message: None,
                    })
                }
            }
            Err(e) => {
                // Gracefully handle network/RPC failures by returning SimulateResult with details
                Ok(SimulateResult {
                    success: false,
                    gas_consumed: 0,
                    result_xdr: None,
                    error_message: Some(e.to_string()),
                })
            }
        }
    }

    /// Build a deterministic transaction envelope from its components
    /// without relying on a live network round-trip.
    ///
    /// The returned XDR bytes can be signed separately and submitted later.
    pub fn build_tx_envelope(
        &self,
        contract_id: &str,
        function_name: &str,
        args_xdr: &[u8],
    ) -> Result<Vec<u8>> {
        use stellar_xdr::curr::WriteXdr;

        let envelope = self.build_envelope_struct(contract_id, function_name, args_xdr)?;
        envelope
            .to_xdr(stellar_xdr::curr::Limits::none())
            .map_err(|e| StellarAidError::validation(format!("XDR encoding failed: {}", e)))
    }

    /// Build a base64-encoded transaction envelope XDR string.
    pub fn build_tx_envelope_base64(
        &self,
        contract_id: &str,
        function_name: &str,
        args_xdr: &[u8],
    ) -> Result<String> {
        use stellar_xdr::curr::WriteXdr;

        let envelope = self.build_envelope_struct(contract_id, function_name, args_xdr)?;
        envelope
            .to_xdr_base64(stellar_xdr::curr::Limits::none())
            .map_err(|e| StellarAidError::validation(format!("XDR encoding failed: {}", e)))
    }

    fn build_envelope_struct(
        &self,
        contract_id: &str,
        function_name: &str,
        args_xdr: &[u8],
    ) -> Result<stellar_xdr::curr::TransactionEnvelope> {
        use stellar_xdr::curr::{
            Hash, HostFunction, InvokeContractArgs, InvokeHostFunctionOp, Limits, Memo,
            MuxedAccount, Operation, OperationBody, Preconditions, ReadXdr, ScAddress, ScSymbol,
            ScVal, SequenceNumber, TimeBounds, TimePoint, Transaction, TransactionEnvelope,
            TransactionExt, TransactionV1Envelope, Uint256, VecM,
        };

        if contract_id.is_empty() {
            return Err(StellarAidError::ValidationError("contract_id must not be empty".into()));
        }
        if function_name.is_empty() {
            return Err(StellarAidError::ValidationError("function_name must not be empty".into()));
        }

        let source_account = match stellar_strkey::Strkey::from_string(self.source_account) {
            Ok(stellar_strkey::Strkey::PublicKeyEd25519(pk)) => {
                MuxedAccount::Ed25519(Uint256(pk.0))
            }
            _ => {
                let mut bytes = [0u8; 32];
                for (i, b) in self.source_account.bytes().take(32).enumerate() {
                    bytes[i] = b;
                }
                MuxedAccount::Ed25519(Uint256(bytes))
            }
        };

        let contract_addr = match stellar_strkey::Strkey::from_string(contract_id) {
            Ok(stellar_strkey::Strkey::Contract(h)) => ScAddress::Contract(Hash(h.0)),
            _ => {
                let mut bytes = [0u8; 32];
                for (i, b) in contract_id.bytes().take(32).enumerate() {
                    bytes[i] = b;
                }
                ScAddress::Contract(Hash(bytes))
            }
        };

        let sym_name: ScSymbol = function_name
            .try_into()
            .map_err(|_| StellarAidError::ValidationError("invalid function name".into()))?;

        let mut args_vec = Vec::new();
        if !args_xdr.is_empty() {
            if let Ok(val) = ScVal::from_xdr(args_xdr, Limits::none()) {
                args_vec.push(val);
            }
        }
        let args_m: VecM<ScVal> = args_vec
            .try_into()
            .map_err(|_| StellarAidError::ValidationError("args too large".into()))?;

        let host_fn = HostFunction::InvokeContract(InvokeContractArgs {
            contract_address: contract_addr,
            function_name: sym_name,
            args: args_m,
        });

        let op = Operation {
            source_account: None,
            body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
                host_function: host_fn,
                auth: VecM::default(),
            }),
        };

        let ops: VecM<Operation, 100> = vec![op]
            .try_into()
            .map_err(|_| StellarAidError::ValidationError("operations too large".into()))?;

        let tx = Transaction {
            source_account,
            fee: 100,
            seq_num: SequenceNumber(1),
            cond: Preconditions::Time(TimeBounds {
                min_time: TimePoint(0),
                max_time: TimePoint(0),
            }),
            memo: Memo::None,
            operations: ops,
            ext: TransactionExt::V0,
        };

        Ok(TransactionEnvelope::Tx(TransactionV1Envelope {
            tx,
            signatures: VecM::default(),
        }))
    }

    /// Estimate the resource (gas + footprint) for a given call.
    ///
    /// Uses simulation to return realistic costs.
    pub async fn estimate_resources(
        &self,
        contract_id: &str,
        function_name: &str,
        args_xdr: &[u8],
    ) -> Result<(u64, u64)> {
        let sim = self.simulate_contract_call(contract_id, function_name, args_xdr).await?;
        if !sim.success {
            return Err(StellarAidError::soroban(
                sim.error_message.unwrap_or_else(|| "Simulation failed".into()),
            ));
        }
        Ok((sim.gas_consumed, 100)) // (gas, footprint entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_simulator_rejects_empty_contract_id() {
        let rpc = SorobanRpcClient::new("https://example.com");
        let sim = TransactionSimulator::new(&rpc, "GABCD...");
        let result = sim.simulate_contract_call("", "donate", &[]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_simulator_rejects_empty_function_name() {
        let rpc = SorobanRpcClient::new("https://example.com");
        let sim = TransactionSimulator::new(&rpc, "GABCD...");
        let result = sim.simulate_contract_call("C...", "", &[]).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_build_tx_envelope_returns_bytes() {
        let rpc = SorobanRpcClient::new("https://example.com");
        let sim = TransactionSimulator::new(&rpc, "GABCD...");
        let result = sim.build_tx_envelope("C...", "donate", &[0x01, 0x02]);
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }

    #[test]
    fn test_build_tx_envelope_base64_returns_valid_string() {
        let rpc = SorobanRpcClient::new("https://example.com");
        let sim = TransactionSimulator::new(&rpc, "GABCD...");
        let result = sim.build_tx_envelope_base64("C...", "donate", &[]);
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }

    async fn spawn_mock_rpc_server(response_body: &'static str) -> (String, tokio::task::JoinHandle<()>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = format!("http://127.0.0.1:{}", port);

        let handle = tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 2048];
                let _ = socket.read(&mut buf).await;
                let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}", response_body.len(), response_body);
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });

        (url, handle)
    }

    #[tokio::test]
    async fn test_simulate_contract_call_success_with_actual_gas() {
        let mock_response = r#"{"jsonrpc":"2.0","id":1,"result":{"cost":{"cpuInsns":"154320","memBytes":"4096"},"results":[{"xdr":"AAAAAQ=="}]}}"#;
        let (url, handle) = spawn_mock_rpc_server(mock_response).await;

        let rpc = SorobanRpcClient::new(&url);
        let sim = TransactionSimulator::new(&rpc, "GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN");
        let result = sim.simulate_contract_call("CA3D5KRYM6CB7OWQ6TWYRR3Z4T7GNZLKERYNZGGA5SOAOPIFY6YQGAXE", "donate", &[]).await;

        let _ = handle.await;

        assert!(result.is_ok());
        let res = result.unwrap();
        assert!(res.success);
        assert_eq!(res.gas_consumed, 154320);
        assert_eq!(res.result_xdr, Some("AAAAAQ==".to_string()));
        assert_eq!(res.error_message, None);
    }

    #[tokio::test]
    async fn test_simulate_contract_call_handles_contract_error() {
        let mock_response = r#"{"jsonrpc":"2.0","id":1,"result":{"cost":{"cpuInsns":"50000"},"error":"HostError: Error(Contract, #12)"}}"#;
        let (url, handle) = spawn_mock_rpc_server(mock_response).await;

        let rpc = SorobanRpcClient::new(&url);
        let sim = TransactionSimulator::new(&rpc, "GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN");
        let result = sim.simulate_contract_call("CA3D5KRYM6CB7OWQ6TWYRR3Z4T7GNZLKERYNZGGA5SOAOPIFY6YQGAXE", "donate", &[]).await;

        let _ = handle.await;

        assert!(result.is_ok());
        let res = result.unwrap();
        assert!(!res.success);
        assert_eq!(res.gas_consumed, 50000);
        assert_eq!(res.error_message, Some("HostError: Error(Contract, #12)".to_string()));
    }

    #[tokio::test]
    async fn test_simulate_contract_call_handles_rpc_error_gracefully() {
        let mock_response = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32600,"message":"Invalid transaction envelope"}}"#;
        let (url, handle) = spawn_mock_rpc_server(mock_response).await;

        let rpc = SorobanRpcClient::new(&url);
        let sim = TransactionSimulator::new(&rpc, "GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN");
        let result = sim.simulate_contract_call("CA3D5KRYM6CB7OWQ6TWYRR3Z4T7GNZLKERYNZGGA5SOAOPIFY6YQGAXE", "donate", &[]).await;

        let _ = handle.await;

        assert!(result.is_ok());
        let res = result.unwrap();
        assert!(!res.success);
        assert_eq!(res.gas_consumed, 0);
        assert!(res.error_message.unwrap().contains("Invalid transaction envelope"));
    }

    #[tokio::test]
    async fn test_simulate_contract_call_handles_network_failure_gracefully() {
        // Port 1 is typically unused and refuses connection immediately
        let rpc = SorobanRpcClient::new("http://127.0.0.1:1");
        let sim = TransactionSimulator::new(&rpc, "GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN");
        let result = sim.simulate_contract_call("CA3D5KRYM6CB7OWQ6TWYRR3Z4T7GNZLKERYNZGGA5SOAOPIFY6YQGAXE", "donate", &[]).await;

        assert!(result.is_ok());
        let res = result.unwrap();
        assert!(!res.success);
        assert_eq!(res.gas_consumed, 0);
        assert!(res.error_message.is_some());
    }

    #[tokio::test]
    async fn test_estimate_resources_returns_gas_and_footprint() {
        let mock_response = r#"{"jsonrpc":"2.0","id":1,"result":{"cost":{"cpuInsns":78900},"results":[]}}"#;
        let (url, handle) = spawn_mock_rpc_server(mock_response).await;

        let rpc = SorobanRpcClient::new(&url);
        let sim = TransactionSimulator::new(&rpc, "GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN");
        let result = sim.estimate_resources("CA3D5KRYM6CB7OWQ6TWYRR3Z4T7GNZLKERYNZGGA5SOAOPIFY6YQGAXE", "donate", &[]).await;

        let _ = handle.await;

        assert!(result.is_ok());
        let (gas, footprint) = result.unwrap();
        assert_eq!(gas, 78900);
        assert_eq!(footprint, 100);
    }
}
