use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use url::Url;

use stellar_baselib::xdr;
use stellar_baselib::xdr::{ReadXdr, WriteXdr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WalletType {
    Freighter,
    Albedo,
    Lobstr,
}

impl WalletType {
    pub fn as_str(self) -> &'static str {
        match self {
            WalletType::Freighter => "freighter",
            WalletType::Albedo => "albedo",
            WalletType::Lobstr => "lobstr",
        }
    }
}

impl FromStr for WalletType {
    type Err = WalletSigningError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "freighter" => Ok(WalletType::Freighter),
            "albedo" => Ok(WalletType::Albedo),
            "lobstr" => Ok(WalletType::Lobstr),
            other => Err(WalletSigningError::UnsupportedWallet(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SigningStatus {
    AwaitingUser,
    Signed,
    Rejected,
    TimedOut,
    Invalid,
}

#[derive(Debug, Clone)]
pub struct PrepareSigningRequest {
    pub wallet: WalletType,
    pub unsigned_xdr: String,
    pub network_passphrase: String,
    pub public_key: Option<String>,
    pub callback_url: Option<String>,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct PreparedSigningFlow {
    pub attempt_id: String,
    pub wallet: WalletType,
    pub status: SigningStatus,
    pub message: String,
    pub request_payload: String,
    pub launch_url: Option<String>,
    pub created_at_unix: u64,
    pub expires_at_unix: u64,
}

#[derive(Debug, Clone)]
pub struct CompleteSigningRequest {
    pub attempt_id: String,
    pub wallet: WalletType,
    pub wallet_response: String,
    pub started_at_unix: u64,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct SigningCompletion {
    pub attempt_id: String,
    pub wallet: WalletType,
    pub status: SigningStatus,
    pub message: String,
    pub signed_xdr: Option<String>,
    pub envelope_xdr: Option<String>,
}

#[derive(Debug, Error)]
pub enum WalletSigningError {
    #[error("unsupported wallet '{0}' (expected: freighter, albedo, lobstr)")]
    UnsupportedWallet(String),
    #[error("invalid unsigned transaction XDR: {0}")]
    InvalidUnsignedXdr(String),
    #[error("invalid signed transaction XDR: {0}")]
    InvalidSignedXdr(String),
    #[error("missing signed XDR in wallet response")]
    MissingSignedXdr,
    #[error("wallet response could not be parsed")]
    InvalidWalletResponse,
    #[error("system clock error: {0}")]
    SystemClock(String),
    #[error("failed to write signing log: {0}")]
    Logging(String),
}

#[derive(Debug)]
pub struct WalletSigningService {
    log_path: PathBuf,
}

impl WalletSigningService {
    pub fn new(log_path: impl AsRef<Path>) -> Self {
        Self {
            log_path: log_path.as_ref().to_path_buf(),
        }
    }

    pub fn prepare_signing(
        &self,
        request: PrepareSigningRequest,
    ) -> Result<PreparedSigningFlow, WalletSigningError> {
        validate_xdr(&request.unsigned_xdr).map_err(WalletSigningError::InvalidUnsignedXdr)?;

        let created_at_unix = now_unix_seconds()?;
        let expires_at_unix = created_at_unix.saturating_add(request.timeout_seconds);
        let attempt_id = format!(
            "{}-{}",
            request.wallet.as_str(),
            now_unix_millis().map_err(WalletSigningError::SystemClock)?
        );

        let (request_payload, launch_url) = build_wallet_payload(&request);
        let result = PreparedSigningFlow {
            attempt_id: attempt_id.clone(),
            wallet: request.wallet,
            status: SigningStatus::AwaitingUser,
            message: "Awaiting user signature".to_string(),
            request_payload,
            launch_url,
            created_at_unix,
            expires_at_unix,
        };

        self.log_event(LogEvent {
            phase: "prepare",
            attempt_id,
            wallet: result.wallet,
            status: result.status.clone(),
            message: result.message.clone(),
            signed_xdr: None,
        })?;

        Ok(result)
    }

    pub fn complete_signing(
        &self,
        request: CompleteSigningRequest,
    ) -> Result<SigningCompletion, WalletSigningError> {
        if is_timed_out(request.started_at_unix, request.timeout_seconds)? {
            let completion = SigningCompletion {
                attempt_id: request.attempt_id.clone(),
                wallet: request.wallet,
                status: SigningStatus::TimedOut,
                message: "Signing request timed out".to_string(),
                signed_xdr: None,
                envelope_xdr: None,
            };
            self.log_event(LogEvent {
                phase: "complete",
                attempt_id: request.attempt_id,
                wallet: completion.wallet,
                status: completion.status.clone(),
                message: completion.message.clone(),
                signed_xdr: None,
            })?;
            return Ok(completion);
        }

        let parsed = parse_wallet_response(request.wallet, &request.wallet_response)?;

        let completion = match parsed {
            WalletResponse::Rejected(reason) => SigningCompletion {
                attempt_id: request.attempt_id.clone(),
                wallet: request.wallet,
                status: SigningStatus::Rejected,
                message: format!("Signing rejected by user: {reason}"),
                signed_xdr: None,
                envelope_xdr: None,
            },
            WalletResponse::TimedOut(reason) => SigningCompletion {
                attempt_id: request.attempt_id.clone(),
                wallet: request.wallet,
                status: SigningStatus::TimedOut,
                message: reason,
                signed_xdr: None,
                envelope_xdr: None,
            },
            WalletResponse::Signed(signed_xdr) => {
                let canonical_envelope = extract_envelope_xdr(&signed_xdr)
                    .map_err(WalletSigningError::InvalidSignedXdr)?;

                SigningCompletion {
                    attempt_id: request.attempt_id.clone(),
                    wallet: request.wallet,
                    status: SigningStatus::Signed,
                    message: "Transaction signed successfully".to_string(),
                    signed_xdr: Some(signed_xdr),
                    envelope_xdr: Some(canonical_envelope),
                }
            },
        };

        self.log_event(LogEvent {
            phase: "complete",
            attempt_id: request.attempt_id,
            wallet: completion.wallet,
            status: completion.status.clone(),
            message: completion.message.clone(),
            signed_xdr: completion.signed_xdr.clone(),
        })?;

        Ok(completion)
    }

    fn log_event(&self, event: LogEvent) -> Result<(), WalletSigningError> {
        let timestamp = now_unix_seconds()?;
        let entry = serde_json::json!({
            "timestamp": timestamp,
            "phase": event.phase,
            "attempt_id": event.attempt_id,
            "wallet": event.wallet.as_str(),
            "status": event.status,
            "message": event.message,
            "has_signed_xdr": event.signed_xdr.is_some(),
        });

        if let Some(parent) = self.log_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| WalletSigningError::Logging(error.to_string()))?;
            }
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
            .map_err(|error| WalletSigningError::Logging(error.to_string()))?;

        file.write_all(entry.to_string().as_bytes())
            .and_then(|_| file.write_all(b"\n"))
            .map_err(|error| WalletSigningError::Logging(error.to_string()))
    }
}

#[derive(Debug)]
struct LogEvent {
    phase: &'static str,
    attempt_id: String,
    wallet: WalletType,
    status: SigningStatus,
    message: String,
    signed_xdr: Option<String>,
}

#[derive(Debug)]
enum WalletResponse {
    Signed(String),
    Rejected(String),
    TimedOut(String),
}

fn build_wallet_payload(request: &PrepareSigningRequest) -> (String, Option<String>) {
    match request.wallet {
        WalletType::Freighter => {
            let payload = serde_json::json!({
                "wallet": "freighter",
                "method": "signTransaction",
                "params": {
                    "xdr": request.unsigned_xdr,
                    "networkPassphrase": request.network_passphrase,
                    "address": request.public_key,
                    "timeoutSeconds": request.timeout_seconds,
                },
            });
            (payload.to_string(), None)
        },
        WalletType::Albedo => {
            let mut params = vec![
                ("xdr", request.unsigned_xdr.clone()),
                ("network_passphrase", request.network_passphrase.clone()),
            ];
            if let Some(pubkey) = request.public_key.clone() {
                params.push(("pubkey", pubkey));
            }
            if let Some(callback_url) = request.callback_url.clone() {
                params.push(("callback", callback_url));
            }
            let url = build_url("https://albedo.link/tx", &params);
            (url.clone(), Some(url))
        },
        WalletType::Lobstr => {
            let mut params = vec![
                ("xdr", request.unsigned_xdr.clone()),
                ("network_passphrase", request.network_passphrase.clone()),
            ];
            if let Some(callback_url) = request.callback_url.clone() {
                params.push(("callback", callback_url));
            }
            let deep_link = build_url("lobstr://sign-transaction", &params);
            (deep_link.clone(), Some(deep_link))
        },
    }
}

fn build_url(base: &str, params: &[(impl AsRef<str>, impl AsRef<str>)]) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in params {
        serializer.append_pair(key.as_ref(), value.as_ref());
    }
    let query = serializer.finish();
    format!("{base}?{query}")
}

fn parse_wallet_response(
    wallet: WalletType,
    response: &str,
) -> Result<WalletResponse, WalletSigningError> {
    let trimmed = response.trim();

    if trimmed.is_empty() {
        return Err(WalletSigningError::InvalidWalletResponse);
    }

    if let Some(status) = parse_rejection_or_timeout(trimmed) {
        return Ok(status);
    }

    let params = parse_parameters(trimmed);
    if let Some(status) = parse_rejection_or_timeout_from_map(&params) {
        return Ok(status);
    }

    if let Some(xdr) = first_present_value(
        &params,
        &["signed_xdr", "xdr", "envelope_xdr", "tx_envelope"],
    ) {
        return Ok(WalletResponse::Signed(xdr));
    }

    if matches!(wallet, WalletType::Freighter) {
        if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
            if let Some(xdr) = value
                .get("signed_xdr")
                .or_else(|| value.get("xdr"))
                .and_then(Value::as_str)
            {
                return Ok(WalletResponse::Signed(xdr.to_string()));
            }
        }

        if validate_xdr(trimmed).is_ok() {
            return Ok(WalletResponse::Signed(trimmed.to_string()));
        }
    }

    if validate_xdr(trimmed).is_ok() {
        return Ok(WalletResponse::Signed(trimmed.to_string()));
    }

    Err(WalletSigningError::MissingSignedXdr)
}

fn parse_parameters(raw: &str) -> HashMap<String, String> {
    if let Ok(json) = serde_json::from_str::<Value>(raw) {
        if let Some(object) = json.as_object() {
            return object
                .iter()
                .map(|(key, value)| {
                    let mapped = value
                        .as_str()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| value.to_string());
                    (key.clone(), mapped)
                })
                .collect();
        }
    }

    if let Ok(url) = Url::parse(raw) {
        return url.query_pairs().into_owned().collect();
    }

    let mut input = raw;
    if let Some(index) = raw.find('?') {
        input = &raw[index + 1..];
    }

    if input.contains('=') {
        return url::form_urlencoded::parse(input.as_bytes())
            .into_owned()
            .collect();
    }

    HashMap::new()
}

fn parse_rejection_or_timeout(value: &str) -> Option<WalletResponse> {
    let normalized = value.to_ascii_lowercase();
    if contains_timeout_word(&normalized) {
        return Some(WalletResponse::TimedOut(
            "Wallet signing timed out".to_string(),
        ));
    }

    if contains_rejection_word(&normalized) {
        return Some(WalletResponse::Rejected(value.to_string()));
    }

    None
}

fn parse_rejection_or_timeout_from_map(params: &HashMap<String, String>) -> Option<WalletResponse> {
    let error_value = first_present_value(params, &["error", "status", "result", "message"])?;

    let normalized = error_value.to_ascii_lowercase();
    if contains_timeout_word(&normalized) {
        return Some(WalletResponse::TimedOut(error_value));
    }

    if contains_rejection_word(&normalized) {
        return Some(WalletResponse::Rejected(error_value));
    }

    None
}

fn first_present_value(params: &HashMap<String, String>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        params
            .get(*key)
            .map(String::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    })
}

fn contains_rejection_word(value: &str) -> bool {
    ["reject", "denied", "declined", "cancelled", "canceled"]
        .iter()
        .any(|needle| value.contains(needle))
}

fn contains_timeout_word(value: &str) -> bool {
    ["timeout", "timed_out", "expired"]
        .iter()
        .any(|needle| value.contains(needle))
}

fn extract_envelope_xdr(signed_xdr: &str) -> Result<String, String> {
    let envelope = xdr::TransactionEnvelope::from_xdr_base64(signed_xdr, xdr::Limits::none())
        .map_err(|error| error.to_string())?;

    envelope
        .to_xdr_base64(xdr::Limits::none())
        .map_err(|error| error.to_string())
}

fn validate_xdr(xdr_base64: &str) -> Result<(), String> {
    xdr::TransactionEnvelope::from_xdr_base64(xdr_base64, xdr::Limits::none())
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn now_unix_seconds() -> Result<u64, WalletSigningError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| WalletSigningError::SystemClock(error.to_string()))
}

fn now_unix_millis() -> Result<u128, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| error.to_string())
}

fn is_timed_out(started_at_unix: u64, timeout_seconds: u64) -> Result<bool, WalletSigningError> {
    let now = now_unix_seconds()?;
    Ok(now.saturating_sub(started_at_unix) > timeout_seconds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::donation_tx_builder::{build_donation_transaction, BuildDonationTxRequest};
    use stellar_baselib::keypair::{Keypair, KeypairBehavior};
    use tempfile::tempdir;

    fn sample_unsigned_xdr() -> String {
        let donor = Keypair::random().unwrap().public_key();
        let destination = Keypair::random().unwrap().public_key();

        let request = BuildDonationTxRequest {
            donor_address: donor,
            donor_sequence: "120".to_string(),
            platform_address: destination,
            donation_amount: "5.25".to_string(),
            asset_code: "XLM".to_string(),
            asset_issuer: None,
            project_id: "wallet_signing".to_string(),
            network_passphrase: "Test SDF Network ; September 2015".to_string(),
            timeout_seconds: 300,
            base_fee_stroops: 100,
        };

        build_donation_transaction(request).unwrap().xdr
    }

    fn service_in_temp_dir() -> WalletSigningService {
        let temp_dir = tempdir().unwrap();
        let log_path = temp_dir.path().join("wallet_signing.log");
        WalletSigningService::new(log_path)
    }

    #[test]
    fn prepares_freighter_signing_payload() {
        let service = service_in_temp_dir();
        let unsigned_xdr = sample_unsigned_xdr();

        let prepared = service
            .prepare_signing(PrepareSigningRequest {
                wallet: WalletType::Freighter,
                unsigned_xdr,
                network_passphrase: "Test SDF Network ; September 2015".to_string(),
                public_key: Some(Keypair::random().unwrap().public_key()),
                callback_url: None,
                timeout_seconds: 120,
            })
            .unwrap();

        assert_eq!(prepared.status, SigningStatus::AwaitingUser);
        assert!(prepared.request_payload.contains("signTransaction"));
        assert!(prepared.launch_url.is_none());
    }

    #[test]
    fn prepares_albedo_signing_url() {
        let service = service_in_temp_dir();
        let unsigned_xdr = sample_unsigned_xdr();

        let prepared = service
            .prepare_signing(PrepareSigningRequest {
                wallet: WalletType::Albedo,
                unsigned_xdr,
                network_passphrase: "Test SDF Network ; September 2015".to_string(),
                public_key: Some(Keypair::random().unwrap().public_key()),
                callback_url: Some("https://app.local/signing/callback".to_string()),
                timeout_seconds: 120,
            })
            .unwrap();

        let launch_url = prepared.launch_url.unwrap();
        assert!(launch_url.starts_with("https://albedo.link/tx?"));
        assert!(launch_url.contains("callback=https%3A%2F%2Fapp.local%2Fsigning%2Fcallback"));
    }

    #[test]
    fn prepares_lobstr_deep_link() {
        let service = service_in_temp_dir();
        let unsigned_xdr = sample_unsigned_xdr();

        let prepared = service
            .prepare_signing(PrepareSigningRequest {
                wallet: WalletType::Lobstr,
                unsigned_xdr,
                network_passphrase: "Test SDF Network ; September 2015".to_string(),
                public_key: None,
                callback_url: Some("https://app.local/signing/callback".to_string()),
                timeout_seconds: 120,
            })
            .unwrap();

        let launch_url = prepared.launch_url.unwrap();
        assert!(launch_url.starts_with("lobstr://sign-transaction?"));
        assert!(launch_url.contains("callback=https%3A%2F%2Fapp.local%2Fsigning%2Fcallback"));
    }

    #[test]
    fn handles_user_rejection_gracefully() {
        let service = service_in_temp_dir();
        let completion = service
            .complete_signing(CompleteSigningRequest {
                attempt_id: "attempt-1".to_string(),
                wallet: WalletType::Freighter,
                wallet_response: "User rejected transaction".to_string(),
                started_at_unix: now_unix_seconds().unwrap(),
                timeout_seconds: 30,
            })
            .unwrap();

        assert_eq!(completion.status, SigningStatus::Rejected);
        assert!(completion.signed_xdr.is_none());
    }

    #[test]
    fn handles_signing_timeout() {
        let service = service_in_temp_dir();
        let completion = service
            .complete_signing(CompleteSigningRequest {
                attempt_id: "attempt-timeout".to_string(),
                wallet: WalletType::Albedo,
                wallet_response: "".to_string(),
                started_at_unix: 0,
                timeout_seconds: 1,
            })
            .unwrap();

        assert_eq!(completion.status, SigningStatus::TimedOut);
        assert!(completion.signed_xdr.is_none());
    }

    #[test]
    fn extracts_signed_xdr_from_albedo_callback() {
        let service = service_in_temp_dir();
        let unsigned_xdr = sample_unsigned_xdr();
        let callback = format!(
            "https://app.local/signing/callback?signed_xdr={}",
            url::form_urlencoded::byte_serialize(unsigned_xdr.as_bytes()).collect::<String>()
        );

        let completion = service
            .complete_signing(CompleteSigningRequest {
                attempt_id: "attempt-albedo".to_string(),
                wallet: WalletType::Albedo,
                wallet_response: callback,
                started_at_unix: now_unix_seconds().unwrap(),
                timeout_seconds: 120,
            })
            .unwrap();

        assert_eq!(completion.status, SigningStatus::Signed);
        assert!(completion.signed_xdr.is_some());
        assert!(completion.envelope_xdr.is_some());
    }

    #[test]
    fn extracts_signed_xdr_from_lobstr_callback() {
        let service = service_in_temp_dir();
        let unsigned_xdr = sample_unsigned_xdr();
        let callback = format!(
            "xdr={}",
            url::form_urlencoded::byte_serialize(unsigned_xdr.as_bytes()).collect::<String>()
        );

        let completion = service
            .complete_signing(CompleteSigningRequest {
                attempt_id: "attempt-lobstr".to_string(),
                wallet: WalletType::Lobstr,
                wallet_response: callback,
                started_at_unix: now_unix_seconds().unwrap(),
                timeout_seconds: 120,
            })
            .unwrap();

        assert_eq!(completion.status, SigningStatus::Signed);
        assert!(completion.signed_xdr.is_some());
    }

    #[test]
    fn rejects_invalid_signed_xdr() {
        let service = service_in_temp_dir();
        let result = service.complete_signing(CompleteSigningRequest {
            attempt_id: "attempt-invalid".to_string(),
            wallet: WalletType::Freighter,
            wallet_response: "not-an-xdr".to_string(),
            started_at_unix: now_unix_seconds().unwrap(),
            timeout_seconds: 120,
        });

        assert!(matches!(result, Err(WalletSigningError::MissingSignedXdr)));
    }
}
