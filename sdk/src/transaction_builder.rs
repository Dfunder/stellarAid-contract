use crate::errors::{Result, StellarAidError};
use crate::horizon::client::HorizonClient;
use crate::soroban::rpc_client::SorobanRpcClient;

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub rpc_url: String,
    pub horizon_url: String,
    pub network_passphrase: String,
}

#[derive(Debug, Clone)]
pub struct DonationParams {
    pub donor: String,
    pub campaign_id: u64,
    pub amount: i128,
    pub token_address: Option<String>,
    pub anonymous: bool,
    pub memo: Option<String>,
    pub donation_contract_id: String,
}

pub async fn build_donate_transaction(
    donor: &str,
    campaign_id: u64,
    amount: i128,
    network: &NetworkConfig,
    donation_contract_id: &str,
) -> Result<String> {
    let params = DonationParams {
        donor: donor.to_string(),
        campaign_id,
        amount,
        token_address: None,
        anonymous: false,
        memo: None,
        donation_contract_id: donation_contract_id.to_string(),
    };
    build_donate_transaction_full(&params, network).await
}

pub async fn build_donate_transaction_full(
    params: &DonationParams,
    network: &NetworkConfig,
) -> Result<String> {
    use stellar_xdr::curr::{
        AccountId, Hash, HostFunction, InvokeContractArgs, InvokeHostFunctionOp, Limits, Memo,
        MuxedAccount, Operation, OperationBody, Preconditions, PublicKey, ScAddress, ScSymbol,
        ScVal, ScVec, SequenceNumber, StringM, TimeBounds, TimePoint, Transaction,
        TransactionEnvelope, TransactionExt, TransactionV1Envelope, Uint256, VecM, WriteXdr,
    };

    let horizon = HorizonClient::new(&network.horizon_url);
    let rpc = SorobanRpcClient::new(&network.rpc_url);

    let account = horizon
        .get_account(&params.donor)
        .await
        .map_err(|e| StellarAidError::horizon(format!("failed to fetch account: {}", e)))?;
    let seq: u64 = account
        .sequence
        .parse()
        .map_err(|_| StellarAidError::horizon("invalid sequence number"))?;

    let donor_pk = stellar_strkey::Strkey::from_string(&params.donor)
        .map_err(|_| StellarAidError::validation("invalid donor address"))?;

    let source_account = match &donor_pk {
        stellar_strkey::Strkey::PublicKeyEd25519(pk) => {
            MuxedAccount::Ed25519(Uint256(pk.0))
        }
        _ => return Err(StellarAidError::validation("donor must be a G... address")),
    };

    let donor_addr = match &donor_pk {
        stellar_strkey::Strkey::PublicKeyEd25519(pk) => {
            ScAddress::Account(AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(pk.0))))
        }
        _ => return Err(StellarAidError::validation("donor must be a G... address")),
    };

    let contract_raw = stellar_strkey::Strkey::from_string(&params.donation_contract_id)
        .map_err(|_| StellarAidError::validation("invalid contract id"))?;

    let contract_addr = match &contract_raw {
        stellar_strkey::Strkey::Contract(h) => ScAddress::Contract(Hash(h.0)),
        _ => return Err(StellarAidError::validation("expected a C... contract id")),
    };

    let sym_donate: ScSymbol = "donate"
        .try_into()
        .map_err(|_| StellarAidError::validation("invalid function name"))?;

    let token_val = match &params.token_address {
        Some(addr) => {
            let raw = stellar_strkey::Strkey::from_string(addr)
                .map_err(|_| StellarAidError::validation("invalid token address"))?;
            match &raw {
                stellar_strkey::Strkey::Contract(h) => {
                    ScVal::Address(ScAddress::Contract(Hash(h.0)))
                }
                _ => return Err(StellarAidError::validation("expected a C... contract id for token")),
            }
        }
        None => ScVal::Address(ScAddress::Contract(Hash([0u8; 32]))),
    };

    let memo_val = match &params.memo {
        Some(m) => {
            let chars: Vec<ScVal> = m.bytes().map(|b| ScVal::U32(b as u32)).collect();
            let char_vec_m = chars
                .try_into()
                .map_err(|_| StellarAidError::validation("memo too long"))?;
            ScVal::Vec(Some(ScVec(char_vec_m)))
        }
        None => ScVal::Void,
    };

    let params_vec = vec![
        ScVal::Address(donor_addr),
        ScVal::U64(params.campaign_id),
        ScVal::I128(stellar_xdr::curr::Int128Parts {
            lo: params.amount as u64,
            hi: (params.amount >> 64) as i64,
        }),
        token_val,
        ScVal::Bool(params.anonymous),
        memo_val,
    ];
    let params_m: VecM<ScVal> = params_vec
        .try_into()
        .map_err(|_| StellarAidError::validation("params too long"))?;

    let host_fn = HostFunction::InvokeContract(InvokeContractArgs {
        contract_address: contract_addr,
        function_name: sym_donate,
        args: params_m,
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
        .map_err(|_| StellarAidError::validation("operations too long"))?;

    let memo_xdr = match &params.memo {
        Some(m) => {
            let s = StringM::try_from(m.as_str())
                .map_err(|_| StellarAidError::validation("invalid memo"))?;
            Memo::Text(s)
        }
        None => Memo::None,
    };

    let tx = Transaction {
        source_account,
        fee: 100_000,
        seq_num: SequenceNumber(seq as i64 + 1),
        cond: Preconditions::Time(TimeBounds {
            min_time: TimePoint(0),
            max_time: TimePoint(0),
        }),
        memo: memo_xdr,
        operations: ops,
        ext: TransactionExt::V0,
    };

    let envelope = TransactionEnvelope::Tx(TransactionV1Envelope {
        tx: tx.clone(),
        signatures: VecM::default(),
    });
    let xdr = envelope
        .to_xdr_base64(Limits::none())
        .map_err(|e| StellarAidError::validation(format!("XDR encoding failed: {}", e)))?;

    let simulation = rpc
        .simulate_transaction(&xdr)
        .await
        .map_err(|e| StellarAidError::soroban(format!("simulation failed: {}", e)))?;

    let sim_fee = simulation
        .cost
        .as_ref()
        .and_then(|c| c.get("minResourceFee").and_then(|v| v.as_u64()))
        .unwrap_or(100_000);

    let tx_final = Transaction {
        fee: (sim_fee + 100) as u32,
        ..tx
    };

    let envelope_final = TransactionEnvelope::Tx(TransactionV1Envelope {
        tx: tx_final,
        signatures: VecM::default(),
    });
    envelope_final
        .to_xdr_base64(Limits::none())
        .map_err(|e| StellarAidError::validation(format!("XDR encoding failed: {}", e)))
}
