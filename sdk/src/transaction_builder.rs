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
    use soroban_sdk::xdr::{
        AccountId, Hash, HostFunction, InvokeHostFunctionOp, Memo, MuxedAccount,
        Operation, OperationBody, Preconditions, PublicKey, ScAddress, ScVal,
        ScVec, SequenceNumber, TimeBounds, Transaction, TransactionEnvelope,
        TransactionExt, Uint256, VecM, WriteXdr,
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
        stellar_strkey::Strkey::ContractId(h) => ScAddress::Contract(Hash(Uint256(h.0))),
        _ => return Err(StellarAidError::validation("expected a C... contract id")),
    };

    let sym_donate = "donate"
        .try_into()
        .map_err(|_| StellarAidError::validation("invalid function name"))?;

    let mut params_sc = ScVec(VecM::default());
    params_sc.0.push(donor_addr);
    params_sc.0.push(ScVal::U64(params.campaign_id));
    params_sc.0.push(ScVal::I128(soroban_sdk::xdr::Int128Parts {
        lo: params.amount as u64,
        hi: (params.amount >> 64) as u64,
    }));

    let token_val = match &params.token_address {
        Some(addr) => {
            let raw = stellar_strkey::Strkey::from_string(addr)
                .map_err(|_| StellarAidError::validation("invalid token address"))?;
            match &raw {
                stellar_strkey::Strkey::ContractId(h) => {
                    ScVal::Address(ScAddress::Contract(Hash(Uint256(h.0))))
                }
                _ => return Err(StellarAidError::validation("expected a C... contract id for token")),
            }
        }
        None => ScVal::Address(ScAddress::Contract(Hash(Uint256([0u8; 32])))),
    };
    params_sc.0.push(token_val);

    params_sc.0.push(ScVal::Bool(params.anonymous));

    let memo_val = match &params.memo {
        Some(m) => {
            let mut chars = VecM::default();
            for b in m.bytes() {
                chars.push(b as i64);
            }
            ScVal::Vec(Some(ScVec(chars)))
        }
        None => ScVal::Void,
    };
    params_sc.0.push(memo_val);

    let host_fn = HostFunction::InvokeHostFunction(InvokeHostFunctionOp {
        contract_id: contract_addr,
        function_name: sym_donate,
        parameters: params_sc,
        auth: VecM::default(),
    });

    let op = Operation {
        source_account: None,
        body: OperationBody::InvokeHostFunction(host_fn),
    };

    let mut ops = VecM::default();
    ops.push(op);

    let memo_xdr = match &params.memo {
        Some(m) => {
            let mut bytes = VecM::default();
            for b in m.bytes() {
                bytes.push(b);
            }
            Memo::MemoText(bytes)
        }
        None => Memo::None,
    };

    let tx = Transaction {
        source_account,
        fee: 100_000,
        seq_num: SequenceNumber(seq as i64 + 1),
        cond: Preconditions::Time(TimeBounds {
            min_time: 0,
            max_time: 0,
        }),
        memo: memo_xdr,
        operations: ops,
        ext: TransactionExt::V0,
    };

    let envelope = TransactionEnvelope::EnvelopeTypeTx(tx);
    let xdr = envelope
        .to_xdr_base64()
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
        fee: sim_fee + 100,
        ..tx
    };

    let envelope_final = TransactionEnvelope::EnvelopeTypeTx(tx_final);
    envelope_final
        .to_xdr_base64()
        .map_err(|e| StellarAidError::validation(format!("XDR encoding failed: {}", e)))
}
