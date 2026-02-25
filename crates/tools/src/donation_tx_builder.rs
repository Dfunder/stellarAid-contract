use thiserror::Error;

use stellar_baselib::account::{Account, AccountBehavior};
use stellar_baselib::asset::{Asset, AssetBehavior};
use stellar_baselib::operation::{Operation, ONE};
use stellar_baselib::transaction::TransactionBehavior;
use stellar_baselib::transaction_builder::{TransactionBuilder, TransactionBuilderBehavior};
use stellar_baselib::xdr;
use stellar_baselib::xdr::WriteXdr;

#[derive(Debug, Clone)]
pub struct BuildDonationTxRequest {
    pub donor_address: String,
    pub donor_sequence: String,
    pub platform_address: String,
    pub donation_amount: String,
    pub asset_code: String,
    pub asset_issuer: Option<String>,
    pub project_id: String,
    pub network_passphrase: String,
    pub timeout_seconds: i64,
    pub base_fee_stroops: u32,
}

#[derive(Debug, Clone)]
pub struct BuildDonationTxResult {
    pub xdr: String,
    pub memo: String,
    pub fee: u32,
    pub amount_stroops: i64,
    pub asset: String,
    pub destination: String,
}

#[derive(Debug, Error)]
pub enum BuildDonationTxError {
    #[error("invalid donor account: {0}")]
    InvalidDonorAccount(String),
    #[error("invalid destination account: {0}")]
    InvalidDestinationAccount(String),
    #[error("invalid amount '{0}' (must be positive and have at most 7 decimals)")]
    InvalidAmount(String),
    #[error("invalid asset: {0}")]
    InvalidAsset(String),
    #[error("project ID cannot be empty")]
    EmptyProjectId,
    #[error("memo is too long for Stellar text memo (max 28 bytes): '{0}'")]
    MemoTooLong(String),
    #[error("memo must be ASCII text")]
    MemoNotAscii,
    #[error("timeout must be non-negative")]
    InvalidTimeout,
    #[error("transaction build failed: {0}")]
    BuildFailed(String),
}

pub fn build_donation_transaction(
    request: BuildDonationTxRequest,
) -> Result<BuildDonationTxResult, BuildDonationTxError> {
    if request.timeout_seconds < 0 {
        return Err(BuildDonationTxError::InvalidTimeout);
    }

    let project_id = request.project_id.trim();
    if project_id.is_empty() {
        return Err(BuildDonationTxError::EmptyProjectId);
    }

    let memo = format!("project_{project_id}");
    if !memo.is_ascii() {
        return Err(BuildDonationTxError::MemoNotAscii);
    }
    if memo.len() > 28 {
        return Err(BuildDonationTxError::MemoTooLong(memo));
    }

    let amount_stroops = parse_amount_to_stroops(&request.donation_amount)?;
    let asset = parse_asset(&request.asset_code, request.asset_issuer.as_deref())?;

    let mut source_account = Account::new(&request.donor_address, &request.donor_sequence)
        .map_err(BuildDonationTxError::InvalidDonorAccount)?;

    let payment = Operation::new()
        .payment(&request.platform_address, &asset, amount_stroops)
        .map_err(|e| match e {
            stellar_baselib::operation::Error::InvalidField(_) => {
                BuildDonationTxError::InvalidDestinationAccount(request.platform_address.clone())
            },
            stellar_baselib::operation::Error::InvalidAmount(_) => {
                BuildDonationTxError::InvalidAmount(request.donation_amount.clone())
            },
            stellar_baselib::operation::Error::InvalidPrice(_, _) => {
                BuildDonationTxError::BuildFailed(
                    "unexpected invalid price for payment op".to_string(),
                )
            },
        })?;

    let mut tx_builder =
        TransactionBuilder::new(&mut source_account, &request.network_passphrase, None);
    tx_builder
        .fee(request.base_fee_stroops)
        .add_operation(payment)
        .add_memo(&memo)
        .set_timeout(request.timeout_seconds)
        .map_err(BuildDonationTxError::BuildFailed)?;

    let transaction = tx_builder.build();
    let envelope = transaction
        .to_envelope()
        .map_err(|e| BuildDonationTxError::BuildFailed(e.to_string()))?;

    let xdr = envelope
        .to_xdr_base64(xdr::Limits::none())
        .map_err(|e| BuildDonationTxError::BuildFailed(e.to_string()))?;

    let fee = request.base_fee_stroops;

    Ok(BuildDonationTxResult {
        xdr,
        memo,
        fee,
        amount_stroops,
        asset: asset.to_string_asset(),
        destination: request.platform_address,
    })
}

fn parse_asset(code: &str, issuer: Option<&str>) -> Result<Asset, BuildDonationTxError> {
    if code.eq_ignore_ascii_case("XLM") {
        return Ok(Asset::native());
    }

    let issuer = issuer
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            BuildDonationTxError::InvalidAsset(
                "issuer is required for non-native assets (e.g., USDC)".to_string(),
            )
        })?;

    Asset::new(code, Some(issuer)).map_err(BuildDonationTxError::InvalidAsset)
}

fn parse_amount_to_stroops(amount: &str) -> Result<i64, BuildDonationTxError> {
    let trimmed = amount.trim();
    if trimmed.is_empty() || trimmed.starts_with('-') {
        return Err(BuildDonationTxError::InvalidAmount(amount.to_string()));
    }

    let normalized = if let Some(stripped) = trimmed.strip_prefix('+') {
        stripped
    } else {
        trimmed
    };

    let mut parts = normalized.split('.');
    let whole_part = parts.next().unwrap_or("0");
    let fractional_part = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return Err(BuildDonationTxError::InvalidAmount(amount.to_string()));
    }

    if !whole_part.is_empty()
        && !whole_part
            .chars()
            .all(|character| character.is_ascii_digit())
    {
        return Err(BuildDonationTxError::InvalidAmount(amount.to_string()));
    }
    if !fractional_part
        .chars()
        .all(|character| character.is_ascii_digit())
        || fractional_part.len() > 7
    {
        return Err(BuildDonationTxError::InvalidAmount(amount.to_string()));
    }

    let whole_value = if whole_part.is_empty() {
        0
    } else {
        whole_part
            .parse::<i64>()
            .map_err(|_| BuildDonationTxError::InvalidAmount(amount.to_string()))?
    };

    let mut fractional_string = fractional_part.to_string();
    while fractional_string.len() < 7 {
        fractional_string.push('0');
    }
    let fractional_value = if fractional_string.is_empty() {
        0
    } else {
        fractional_string
            .parse::<i64>()
            .map_err(|_| BuildDonationTxError::InvalidAmount(amount.to_string()))?
    };

    let stroops = whole_value
        .checked_mul(ONE)
        .and_then(|value| value.checked_add(fractional_value))
        .ok_or_else(|| BuildDonationTxError::InvalidAmount(amount.to_string()))?;

    if stroops <= 0 {
        return Err(BuildDonationTxError::InvalidAmount(amount.to_string()));
    }

    Ok(stroops)
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_baselib::keypair::{Keypair, KeypairBehavior};
    use stellar_baselib::xdr::ReadXdr;

    fn sample_request(asset_code: &str, asset_issuer: Option<String>) -> BuildDonationTxRequest {
        let donor = Keypair::random().unwrap().public_key();
        let destination = Keypair::random().unwrap().public_key();

        BuildDonationTxRequest {
            donor_address: donor,
            donor_sequence: "100".to_string(),
            platform_address: destination,
            donation_amount: "12.3456789".to_string(),
            asset_code: asset_code.to_string(),
            asset_issuer,
            project_id: "123".to_string(),
            network_passphrase: "Test SDF Network ; September 2015".to_string(),
            timeout_seconds: 300,
            base_fee_stroops: 100,
        }
    }

    #[test]
    fn builds_native_xlm_transaction() {
        let request = sample_request("XLM", None);
        let result = build_donation_transaction(request).expect("native tx should build");

        assert_eq!(result.memo, "project_123");
        assert_eq!(result.fee, 100);
        assert_eq!(result.amount_stroops, 123_456_789);
        assert_eq!(result.asset, "native");
        assert!(!result.xdr.is_empty());

        let envelope =
            xdr::TransactionEnvelope::from_xdr_base64(&result.xdr, xdr::Limits::none()).unwrap();
        match envelope {
            xdr::TransactionEnvelope::Tx(envelope) => {
                assert_eq!(envelope.tx.fee, 100);
                assert_eq!(envelope.tx.operations.len(), 1);
                match envelope.tx.memo {
                    xdr::Memo::Text(text) => assert_eq!(text.to_string(), "project_123"),
                    _ => panic!("expected text memo"),
                }

                match &envelope.tx.operations[0].body {
                    xdr::OperationBody::Payment(payment) => {
                        assert_eq!(payment.amount, 123_456_789);
                        assert!(matches!(payment.asset, xdr::Asset::Native));
                    },
                    _ => panic!("expected payment operation"),
                }
            },
            _ => panic!("expected tx envelope"),
        }
    }

    #[test]
    fn builds_credit_asset_transaction() {
        let issuer = Keypair::random().unwrap().public_key();
        let request = sample_request("USDC", Some(issuer));
        let result = build_donation_transaction(request).expect("credit tx should build");

        assert_eq!(result.memo, "project_123");
        assert_eq!(result.fee, 100);
        assert!(result.asset.starts_with("USDC:"));

        let envelope =
            xdr::TransactionEnvelope::from_xdr_base64(&result.xdr, xdr::Limits::none()).unwrap();
        match envelope {
            xdr::TransactionEnvelope::Tx(envelope) => match &envelope.tx.operations[0].body {
                xdr::OperationBody::Payment(payment) => {
                    assert!(matches!(payment.asset, xdr::Asset::CreditAlphanum4(_)));
                },
                _ => panic!("expected payment operation"),
            },
            _ => panic!("expected tx envelope"),
        }
    }

    #[test]
    fn builds_credit_asset_12_transaction() {
        let issuer = Keypair::random().unwrap().public_key();
        let request = sample_request("TOKENASSET12", Some(issuer));
        let result = build_donation_transaction(request).expect("credit-12 tx should build");

        let envelope =
            xdr::TransactionEnvelope::from_xdr_base64(&result.xdr, xdr::Limits::none()).unwrap();
        match envelope {
            xdr::TransactionEnvelope::Tx(envelope) => match &envelope.tx.operations[0].body {
                xdr::OperationBody::Payment(payment) => {
                    assert!(matches!(payment.asset, xdr::Asset::CreditAlphanum12(_)));
                },
                _ => panic!("expected payment operation"),
            },
            _ => panic!("expected tx envelope"),
        }
    }

    #[test]
    fn rejects_non_native_asset_without_issuer() {
        let request = sample_request("USDC", None);
        let error = build_donation_transaction(request).unwrap_err();
        assert!(matches!(error, BuildDonationTxError::InvalidAsset(_)));
    }

    #[test]
    fn rejects_invalid_amount_precision() {
        let mut request = sample_request("XLM", None);
        request.donation_amount = "1.12345678".to_string();
        let error = build_donation_transaction(request).unwrap_err();
        assert!(matches!(error, BuildDonationTxError::InvalidAmount(_)));
    }
}
