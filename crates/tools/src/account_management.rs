use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use crate::horizon_client::HorizonClient;
use crate::config::Config;
use crate::secure_vault::SecureVault;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub account_id: String,
    pub public_key: String,
    pub sequence_number: String,
    pub balance: HashMap<String, f64>,
    pub num_subentries: u32,
    pub thresholds: AccountThresholds,
    pub signers: Vec<SignerInfo>,
    pub flags: AccountFlags,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_modified: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountThresholds {
    pub low_threshold: u8,
    pub medium_threshold: u8,
    pub high_threshold: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignerInfo {
    pub key: String,
    pub weight: u8,
    pub type_field: SignerType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignerType {
    PublicKey,
    PreAuthTx,
    HashX,
    Ed25519SignedPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountFlags {
    pub auth_required: bool,
    pub auth_revocable: bool,
    pub auth_immutable: bool,
    pub auth_clawback_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletInfo {
    pub name: String,
    pub wallet_type: WalletType,
    pub accounts: Vec<AccountInfo>,
    pub is_connected: bool,
    pub last_used: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WalletType {
    Freighter,
    Albedo,
    Lobstr,
    HardwareLedger,
    HardwareTrezor,
    PrivateKey,
    Mnemonic,
}

#[derive(Debug, Clone)]
pub struct AccountManagementRequest {
    pub action: AccountAction,
    pub account_id: Option<String>,
    pub wallet_type: Option<WalletType>,
    pub private_key: Option<String>,
    pub mnemonic: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AccountAction {
    Create,
    Import,
    Export,
    List,
    Balance,
    Signers,
    SetOptions,
    Fund,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountCreationResult {
    pub account_id: String,
    public_key: String,
    secret_key: String,
    mnemonic: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountBalance {
    pub asset_code: String,
    pub asset_issuer: Option<String>,
    pub balance: f64,
    pub limit: Option<f64>,
    pub is_authorized: bool,
    pub is_auth_required: bool,
    pub is_auth_revocable: bool,
}

pub struct AccountManagementService;

impl AccountManagementService {
    pub async fn manage_accounts(
        client: &HorizonClient,
        request: AccountManagementRequest,
    ) -> Result<serde_json::Value> {
        match request.action {
            AccountAction::Create => {
                self::create_account(request).await
            },
            AccountAction::Import => {
                self::import_account(request).await
            },
            AccountAction::Export => {
                self::export_account(request).await
            },
            AccountAction::List => {
                self::list_accounts(request).await
            },
            AccountAction::Balance => {
                self::get_account_balance(client, request).await
            },
            AccountAction::Signers => {
                self::get_account_signers(client, request).await
            },
            AccountAction::SetOptions => {
                self::set_account_options(client, request).await
            },
            AccountAction::Fund => {
                self::fund_account(client, request).await
            },
        }
    }
    
    async fn create_account(request: AccountManagementRequest) -> Result<serde_json::Value> {
        let keypair = stellar_baselib::keypair::KeyPair::random()?;
        let public_key = keypair.public_key().to_string();
        let secret_key = keypair.secret().to_string();
        
        let mnemonic = if request.mnemonic.is_some() {
            request.mnemonic
        } else {
            // Generate new mnemonic
            Some(generate_mnemonic()?)
        };
        
        let result = AccountCreationResult {
            account_id: public_key.clone(),
            public_key,
            secret_key,
            mnemonic,
            created_at: chrono::Utc::now(),
        };
        
        // Save to secure vault if password provided
        if let Some(password) = request.password {
            let vault = SecureVault::new(password)?;
            vault.store_account(&result)?;
        }
        
        Ok(serde_json::to_value(result)?)
    }
    
    async fn import_account(request: AccountManagementRequest) -> Result<serde_json::Value> {
        let account_info = if let Some(private_key) = request.private_key {
            let keypair = stellar_baselib::keypair::KeyPair::from_secret_key(&private_key)?;
            let public_key = keypair.public_key().to_string();
            
            AccountCreationResult {
                account_id: public_key.clone(),
                public_key,
                secret_key: private_key,
                mnemonic: None,
                created_at: chrono::Utc::now(),
            }
        } else if let Some(mnemonic) = request.mnemonic {
            // Derive keypair from mnemonic
            let keypair = derive_keypair_from_mnemonic(&mnemonic)?;
            let public_key = keypair.public_key().to_string();
            let secret_key = keypair.secret().to_string();
            
            AccountCreationResult {
                account_id: public_key.clone(),
                public_key,
                secret_key,
                mnemonic: Some(mnemonic),
                created_at: chrono::Utc::now(),
            }
        } else {
            return Err(anyhow::anyhow!("Either private key or mnemonic required for import"));
        };
        
        // Save to secure vault if password provided
        if let Some(password) = request.password {
            let vault = SecureVault::new(password)?;
            vault.store_account(&account_info)?;
        }
        
        Ok(serde_json::to_value(account_info)?)
    }
    
    async fn export_account(request: AccountManagementRequest) -> Result<serde_json::Value> {
        let account_id = request.account_id
            .ok_or_else(|| anyhow::anyhow!("Account ID required for export"))?;
        let password = request.password
            .ok_or_else(|| anyhow::anyhow!("Password required for export"))?;
        
        let vault = SecureVault::new(password)?;
        let account_info = vault.retrieve_account(&account_id)?;
        
        Ok(serde_json::to_value(account_info)?)
    }
    
    async fn list_accounts(_request: AccountManagementRequest) -> Result<serde_json::Value> {
        // List accounts from secure vault or wallet connections
        let accounts = Vec::new(); // Placeholder
        
        Ok(serde_json::json!({
            "accounts": accounts,
            "total_count": accounts.len()
        }))
    }
    
    async fn get_account_balance(
        client: &HorizonClient,
        request: AccountManagementRequest,
    ) -> Result<serde_json::Value> {
        let account_id = request.account_id
            .ok_or_else(|| anyhow::anyhow!("Account ID required for balance query"))?;
        
        let url = format!("/accounts/{}", account_id);
        let response = client.get(&url).await?;
        let account_data: serde_json::Value = serde_json::from_str(&response)?;
        
        let mut balances = Vec::new();
        if let Some(balances_array) = account_data["balances"].as_array() {
            for balance in balances_array {
                balances.push(AccountBalance {
                    asset_code: balance["asset_code"].as_str().unwrap_or("XLM").to_string(),
                    asset_issuer: balance["asset_issuer"].as_str().map(|s| s.to_string()),
                    balance: balance["balance"].as_str().unwrap_or("0").parse::<f64>()?,
                    limit: balance["limit"].as_str().and_then(|s| s.parse::<f64>().ok()),
                    is_authorized: balance["is_authorized"].as_bool().unwrap_or(true),
                    is_auth_required: balance["is_auth_required"].as_bool().unwrap_or(false),
                    is_auth_revocable: balance["is_auth_revocable"].as_bool().unwrap_or(false),
                });
            }
        }
        
        Ok(serde_json::json!({
            "account_id": account_id,
            "sequence": account_data["sequence"].as_str().unwrap_or("0"),
            "balances": balances,
            "total_balance_xlm": balances.iter()
                .find(|b| b.asset_code == "XLM")
                .map(|b| b.balance)
                .unwrap_or(0.0)
        }))
    }
    
    async fn get_account_signers(
        client: &HorizonClient,
        request: AccountManagementRequest,
    ) -> Result<serde_json::Value> {
        let account_id = request.account_id
            .ok_or_else(|| anyhow::anyhow!("Account ID required for signers query"))?;
        
        let url = format!("/accounts/{}", account_id);
        let response = client.get(&url).await?;
        let account_data: serde_json::Value = serde_json::from_str(&response)?;
        
        let mut signers = Vec::new();
        if let Some(signers_array) = account_data["signers"].as_array() {
            for signer in signers_array {
                signers.push(SignerInfo {
                    key: signer["key"].as_str().unwrap_or("").to_string(),
                    weight: signer["weight"].as_u64().unwrap_or(0) as u8,
                    type_field: match signer["type"].as_str().unwrap_or("") {
                        "ed25519_public_key" => SignerType::PublicKey,
                        "preauth_tx" => SignerType::PreAuthTx,
                        "hash_x" => SignerType::HashX,
                        "ed25519_signed_payload" => SignerType::Ed25519SignedPayload,
                        _ => SignerType::PublicKey,
                    },
                });
            }
        }
        
        Ok(serde_json::json!({
            "account_id": account_id,
            "signers": signers,
            "thresholds": {
                "low": account_data["thresholds"]["low_threshold"].as_u64().unwrap_or(0),
                "medium": account_data["thresholds"]["med_threshold"].as_u64().unwrap_or(0),
                "high": account_data["thresholds"]["high_threshold"].as_u64().unwrap_or(0),
            }
        }))
    }
    
    async fn set_account_options(
        client: &HorizonClient,
        request: AccountManagementRequest,
    ) -> Result<serde_json::Value> {
        let account_id = request.account_id
            .ok_or_else(|| anyhow::anyhow!("Account ID required for setting options"))?;
        
        // This would build and submit an options transaction
        // For now, return placeholder
        Ok(serde_json::json!({
            "account_id": account_id,
            "status": "options_set",
            "message": "Account options would be set here"
        }))
    }
    
    async fn fund_account(
        client: &HorizonClient,
        request: AccountManagementRequest,
    ) -> Result<serde_json::Value> {
        let account_id = request.account_id
            .ok_or_else(|| anyhow::anyhow!("Account ID required for funding"))?;
        
        // This would use friendbot on testnet or require manual funding on mainnet
        if client.server_url().contains("testnet") {
            let friendbot_url = format!("https://friendbot.stellar.org?addr={}", account_id);
            let response = reqwest::get(&friendbot_url).await?;
            
            if response.status().is_success() {
                Ok(serde_json::json!({
                    "account_id": account_id,
                    "status": "funded",
                    "source": "friendbot"
                }))
            } else {
                Err(anyhow::anyhow!("Failed to fund account via friendbot"))
            }
        } else {
            Err(anyhow::anyhow!("Manual funding required for mainnet accounts"))
        }
    }
    
    pub async fn connect_wallet(wallet_type: WalletType) -> Result<WalletInfo> {
        match wallet_type {
            WalletType::Freighter => self::connect_freighter().await,
            WalletType::Albedo => self::connect_albedo().await,
            WalletType::Lobstr => self::connect_lobstr().await,
            WalletType::HardwareLedger => self::connect_ledger().await,
            WalletType::HardwareTrezor => self::connect_trezor().await,
            _ => Err(anyhow::anyhow!("Unsupported wallet type")),
        }
    }
    
    async fn connect_freighter() -> Result<WalletInfo> {
        // Check if Freighter is installed and get account info
        // This would involve checking for the Freighter extension
        Ok(WalletInfo {
            name: "Freighter".to_string(),
            wallet_type: WalletType::Freighter,
            accounts: Vec::new(),
            is_connected: false,
            last_used: chrono::Utc::now(),
        })
    }
    
    async fn connect_albedo() -> Result<WalletInfo> {
        // Check if Albedo is available and get account info
        Ok(WalletInfo {
            name: "Albedo".to_string(),
            wallet_type: WalletType::Albedo,
            accounts: Vec::new(),
            is_connected: false,
            last_used: chrono::Utc::now(),
        })
    }
    
    async fn connect_lobstr() -> Result<WalletInfo> {
        // Check if Lobstr is available and get account info
        Ok(WalletInfo {
            name: "LOBSTR".to_string(),
            wallet_type: WalletType::Lobstr,
            accounts: Vec::new(),
            is_connected: false,
            last_used: chrono::Utc::now(),
        })
    }
    
    async fn connect_ledger() -> Result<WalletInfo> {
        // Connect to Ledger hardware wallet
        Ok(WalletInfo {
            name: "Ledger".to_string(),
            wallet_type: WalletType::HardwareLedger,
            accounts: Vec::new(),
            is_connected: false,
            last_used: chrono::Utc::now(),
        })
    }
    
    async fn connect_trezor() -> Result<WalletInfo> {
        // Connect to Trezor hardware wallet
        Ok(WalletInfo {
            name: "Trezor".to_string(),
            wallet_type: WalletType::HardwareTrezor,
            accounts: Vec::new(),
            is_connected: false,
            last_used: chrono::Utc::now(),
        })
    }
    
    pub fn validate_address(address: &str) -> Result<bool> {
        // Validate Stellar address format
        if address.len() != 56 || !address.starts_with('G') {
            return Ok(false);
        }
        
        // Use stellar-baselib for proper validation
        match stellar_baselib::strkey::StrKey::parse_stellar_account(address) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
    
    pub fn estimate_account_creation_fee() -> u32 {
        // Estimate the fee for creating a new account
        // Base reserve + operations
        2000000 // 2 XLM in stroops
    }
}

fn generate_mnemonic() -> Result<String> {
    // Generate a 12-word mnemonic phrase
    use rand::Rng;
    let words = vec![
        "abandon", "ability", "able", "about", "above", "absent", "absorb", "abstract",
        "absurd", "abuse", "access", "accident", "account", "accuse", "achieve", "acid",
        "acoustic", "acquire", "across", "act", "action", "actor", "actress", "actual",
    ];
    
    let mut rng = rand::thread_rng();
    let mut mnemonic = Vec::new();
    
    for _ in 0..12 {
        let word = words[rng.gen_range(0..words.len())];
        mnemonic.push(word);
    }
    
    Ok(mnemonic.join(" "))
}

fn derive_keypair_from_mnemonic(mnemonic: &str) -> Result<stellar_baselib::keypair::KeyPair> {
    // Derive keypair from mnemonic phrase
    // This is a simplified implementation
    // In practice, you'd use proper BIP39 derivation
    let seed = mnemonic.as_bytes();
    let keypair = stellar_baselib::keypair::KeyPair::from_bytes(seed)?;
    Ok(keypair)
}
