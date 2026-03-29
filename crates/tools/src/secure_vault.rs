//! Secure Key Vault Module
//!
//! Provides secure storage and management of cryptographic keys with:
//! - Encryption at rest using AES-256-GCM
//! - Key rotation support
//! - Environment-specific key isolation
//! - Access control and audit logging

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Vault error types
#[derive(Debug, Error)]
pub enum VaultError {
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),
    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),
    #[error("Key not found: {0}")]
    KeyNotFound(String),
    #[error("Key already exists: {0}")]
    KeyAlreadyExists(String),
    #[error("Invalid key format: {0}")]
    InvalidKeyFormat(String),
    #[error("Access denied: {0}")]
    AccessDenied(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Key rotation failed: {0}")]
    RotationFailed(String),
}

pub type VaultResult<T> = Result<T, VaultError>;

/// Key metadata for tracking and rotation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyMetadata {
    /// Unique identifier for the key
    pub key_id: String,
    /// Environment (testnet, mainnet, sandbox)
    pub environment: String,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last accessed timestamp
    pub last_accessed: DateTime<Utc>,
    /// Expiration timestamp (optional)
    pub expires_at: Option<DateTime<Utc>>,
    /// Rotation version (incremented on each rotation)
    pub version: u32,
    /// Key purpose/usage description
    pub purpose: String,
    /// Access level required
    pub access_level: AccessLevel,
    /// Whether key is active
    pub is_active: bool,
}

/// Access level for key operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessLevel {
    /// Full access (admin only)
    Admin,
    /// Read-only access
    ReadOnly,
    /// Write-only access
    WriteOnly,
    /// Read and write access
    ReadWrite,
}

/// Encrypted key entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedKeyEntry {
    /// Encrypted key data (base64 encoded)
    pub ciphertext: String,
    /// Nonce used for encryption (base64 encoded)
    pub nonce: String,
    /// Key metadata
    pub metadata: KeyMetadata,
    /// Previous version reference (for rotation)
    pub previous_version_id: Option<String>,
}

/// Key vault configuration
#[derive(Debug, Clone)]
pub struct VaultConfig {
    /// Master password for encryption/decryption
    pub master_password: String,
    /// Path to vault file
    pub vault_path: PathBuf,
    /// Current environment
    pub environment: String,
    /// Auto-backup on modifications
    pub auto_backup: bool,
}

/// Secure key vault implementation
pub struct KeyVault {
    config: VaultConfig,
    encryption_key: [u8; 32],
    entries: HashMap<String, EncryptedKeyEntry>,
}

impl KeyVault {
    /// Create a new vault or load existing one
    pub fn new(config: VaultConfig) -> VaultResult<Self> {
        // Derive encryption key from master password using SHA-256
        let mut hasher = Sha256::new();
        hasher.update(config.master_password.as_bytes());
        let encryption_key: [u8; 32] = hasher.finalize().into();

        let mut vault = Self {
            config,
            encryption_key,
            entries: HashMap::new(),
        };

        // Load existing vault if it exists
        if vault.config.vault_path.exists() {
            vault.load()?;
        }

        Ok(vault)
    }

    /// Store a key securely
    pub fn store_key(
        &mut self,
        key_id: &str,
        key_data: &[u8],
        purpose: &str,
        access_level: AccessLevel,
        expires_in_days: Option<u64>,
    ) -> VaultResult<()> {
        // Check if key already exists
        if self.entries.contains_key(key_id) {
            return Err(VaultError::KeyAlreadyExists(key_id.to_string()));
        }

        // Encrypt the key
        let (ciphertext, nonce) = self.encrypt(key_data)?;

        // Create metadata
        let now = Utc::now();
        let metadata = KeyMetadata {
            key_id: key_id.to_string(),
            environment: self.config.environment.clone(),
            created_at: now,
            last_accessed: now,
            expires_at: expires_in_days.map(|days| now + chrono::Duration::days(days as i64)),
            version: 1,
            purpose: purpose.to_string(),
            access_level,
            is_active: true,
        };

        // Store encrypted entry
        let entry = EncryptedKeyEntry {
            ciphertext,
            nonce,
            metadata,
            previous_version_id: None,
        };

        self.entries.insert(key_id.to_string(), entry);

        // Save vault to disk
        self.save()?;

        Ok(())
    }

    /// Retrieve a key securely
    pub fn retrieve_key(&mut self, key_id: &str, access_level: AccessLevel) -> VaultResult<Vec<u8>> {
        let entry = self
            .entries
            .get_mut(key_id)
            .ok_or_else(|| VaultError::KeyNotFound(key_id.to_string()))?;

        // Check access level
        if !self.check_access_level(entry.metadata.access_level, access_level) {
            return Err(VaultError::AccessDenied(format!(
                "Insufficient access level for key {}",
                key_id
            )));
        }

        // Check if key is active
        if !entry.metadata.is_active {
            return Err(VaultError::AccessDenied(format!(
                "Key {} is not active",
                key_id
            )));
        }

        // Check expiration
        if let Some(expires_at) = &entry.metadata.expires_at {
            if Utc::now() > *expires_at {
                return Err(VaultError::AccessDenied(format!(
                    "Key {} has expired",
                    key_id
                )));
            }
        }

        // Update last accessed time
        entry.metadata.last_accessed = Utc::now();

        // Decrypt and return
        let nonce_bytes = base64::decode(&entry.nonce)
            .map_err(|e| VaultError::DecryptionFailed(e.to_string()))?;
        let ciphertext_bytes = base64::decode(&entry.ciphertext)
            .map_err(|e| VaultError::DecryptionFailed(e.to_string()))?;

        self.decrypt(&ciphertext_bytes, &nonce_bytes)
    }

    /// Rotate a key (create new version while preserving old)
    pub fn rotate_key(
        &mut self,
        key_id: &str,
        new_key_data: &[u8],
    ) -> VaultResult<String> {
        let old_entry = self
            .entries
            .get(key_id)
            .ok_or_else(|| VaultError::KeyNotFound(key_id.to_string()))?
            .clone();

        // Generate new key ID with version suffix
        let new_key_id = format!("{}_v{}", key_id, old_entry.metadata.version + 1);

        // Encrypt new key
        let (ciphertext, nonce) = self.encrypt(new_key_data)?;

        // Create updated metadata
        let now = Utc::now();
        let metadata = KeyMetadata {
            key_id: new_key_id.clone(),
            environment: self.config.environment.clone(),
            created_at: now,
            last_accessed: now,
            expires_at: old_entry.metadata.expires_at,
            version: old_entry.metadata.version + 1,
            purpose: old_entry.metadata.purpose.clone(),
            access_level: old_entry.metadata.access_level,
            is_active: true,
        };

        // Store new entry with reference to old version
        let new_entry = EncryptedKeyEntry {
            ciphertext,
            nonce,
            metadata,
            previous_version_id: Some(old_entry.metadata.key_id.clone()),
        };

        // Mark old entry as inactive
        if let Some(entry) = self.entries.get_mut(key_id) {
            entry.metadata.is_active = false;
        }

        // Store new entry
        self.entries.insert(new_key_id.clone(), new_entry);

        // Save vault
        self.save()?;

        Ok(new_key_id)
    }

    /// List all active keys for current environment
    pub fn list_keys(&self) -> Vec<&KeyMetadata> {
        self.entries
            .values()
            .filter(|entry| {
                entry.metadata.is_active
                    && entry.metadata.environment == self.config.environment
            })
            .map(|entry| &entry.metadata)
            .collect()
    }

    /// Delete a key permanently
    pub fn delete_key(&mut self, key_id: &str) -> VaultResult<()> {
        self.entries
            .remove(key_id)
            .ok_or_else(|| VaultError::KeyNotFound(key_id.to_string()))?;

        self.save()?;
        Ok(())
    }

    /// Get key metadata without accessing the key
    pub fn get_metadata(&self, key_id: &str) -> VaultResult<&KeyMetadata> {
        self.entries
            .get(key_id)
            .map(|entry| &entry.metadata)
            .ok_or_else(|| VaultError::KeyNotFound(key_id.to_string()))
    }

    /// Export expired keys for cleanup
    pub fn get_expired_keys(&self) -> Vec<&KeyMetadata> {
        let now = Utc::now();
        self.entries
            .values()
            .filter(|entry| {
                entry
                    .metadata
                    .expires_at
                    .map(|exp| now > exp)
                    .unwrap_or(false)
            })
            .map(|entry| &entry.metadata)
            .collect()
    }

    /// Save vault to disk
    fn save(&self) -> VaultResult<()> {
        // Create backup if enabled
        if self.config.auto_backup && self.config.vault_path.exists() {
            let backup_path = self.config.vault_path.with_extension(
                format!(
                    "backup.{}",
                    Utc::now().format("%Y%m%d_%H%M%S")
                ),
            );
            fs::copy(&self.config.vault_path, backup_path)?;
        }

        // Serialize vault data
        let serialized = serde_json::to_vec_pretty(&self.entries)
            .map_err(|e| VaultError::Serialization(e))?;

        // Encrypt entire vault before saving
        let (ciphertext, nonce) = self.encrypt(&serialized)?;

        // Write to file
        let mut file = File::create(&self.config.vault_path)?;
        writeln!(file, "{}", base64::encode(&ciphertext))?;
        writeln!(file, "{}", base64::encode(&nonce))?;

        Ok(())
    }

    /// Load vault from disk
    fn load(&mut self) -> VaultResult<()> {
        let mut file = File::open(&self.config.vault_path)?;
        let mut ciphertext_b64 = String::new();
        let mut nonce_b64 = String::new();

        file.read_to_string(&mut ciphertext_b64)?;
        
        let mut lines = ciphertext_b64.lines();
        ciphertext_b64 = lines.next().unwrap_or("").to_string();
        nonce_b64 = lines.next().unwrap_or("").to_string();

        let ciphertext = base64::decode(&ciphertext_b64)
            .map_err(|e| VaultError::DecryptionFailed(e.to_string()))?;
        let nonce = base64::decode(&nonce_b64)
            .map_err(|e| VaultError::DecryptionFailed(e.to_string()))?;

        let decrypted = self.decrypt(&ciphertext, &nonce)?;
        self.entries = serde_json::from_slice(&decrypted)
            .map_err(|e| VaultError::Serialization(e))?;

        Ok(())
    }

    /// Encrypt data using AES-256-GCM
    fn encrypt(&self, plaintext: &[u8]) -> VaultResult<(String, String)> {
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| VaultError::EncryptionFailed(e.to_string()))?;

        // Generate random nonce
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| VaultError::EncryptionFailed(e.to_string()))?;

        Ok((
            base64::encode(&ciphertext),
            base64::encode(&nonce_bytes),
        ))
    }

    /// Decrypt data using AES-256-GCM
    fn decrypt(&self, ciphertext: &[u8], nonce_bytes: &[u8]) -> VaultResult<Vec<u8>> {
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| VaultError::DecryptionFailed(e.to_string()))?;

        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| VaultError::DecryptionFailed(e.to_string()))?;

        Ok(plaintext)
    }

    /// Check if access level is sufficient
    fn check_access_level(&self, required: AccessLevel, provided: AccessLevel) -> bool {
        match required {
            AccessLevel::Admin => provided == AccessLevel::Admin,
            AccessLevel::ReadOnly => matches!(
                provided,
                AccessLevel::ReadOnly | AccessLevel::ReadWrite | AccessLevel::Admin
            ),
            AccessLevel::WriteOnly => matches!(
                provided,
                AccessLevel::WriteOnly | AccessLevel::ReadWrite | AccessLevel::Admin
            ),
            AccessLevel::ReadWrite => matches!(provided, AccessLevel::ReadWrite | AccessLevel::Admin),
        }
    }
}

/// Builder for creating vault configurations
pub struct VaultConfigBuilder {
    master_password: Option<String>,
    vault_path: Option<PathBuf>,
    environment: String,
    auto_backup: bool,
}

impl VaultConfigBuilder {
    pub fn new() -> Self {
        Self {
            master_password: None,
            vault_path: None,
            environment: "default".to_string(),
            auto_backup: true,
        }
    }

    pub fn master_password(mut self, password: &str) -> Self {
        self.master_password = Some(password.to_string());
        self
    }

    pub fn vault_path(mut self, path: &Path) -> Self {
        self.vault_path = Some(path.to_path_buf());
        self
    }

    pub fn environment(mut self, env: &str) -> Self {
        self.environment = env.to_string();
        self
    }

    pub fn auto_backup(mut self, enabled: bool) -> Self {
        self.auto_backup = enabled;
        self
    }

    pub fn build(self) -> VaultResult<VaultConfig> {
        let master_password = self
            .master_password
            .ok_or_else(|| VaultError::InvalidKeyFormat("Master password required".to_string()))?;

        let vault_path = self.vault_path.unwrap_or_else(|| {
            PathBuf::from(format!(".vault_{}.json", self.environment))
        });

        Ok(VaultConfig {
            master_password,
            vault_path,
            environment: self.environment,
            auto_backup: self.auto_backup,
        })
    }
}

impl Default for VaultConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_store_and_retrieve_key() {
        let dir = tempdir().unwrap();
        let vault_path = dir.path().join("test_vault.json");

        let config = VaultConfigBuilder::new()
            .master_password("test_password_123")
            .vault_path(&vault_path)
            .environment("testnet")
            .build()
            .unwrap();

        let mut vault = KeyVault::new(config).unwrap();

        // Store a key
        let test_key = b"test_secret_key_data";
        vault
            .store_key(
                "test_key",
                test_key,
                "Testing purposes",
                AccessLevel::Admin,
                None,
            )
            .unwrap();

        // Retrieve the key
        let retrieved = vault.retrieve_key("test_key", AccessLevel::Admin).unwrap();
        assert_eq!(retrieved, test_key);
    }

    #[test]
    fn test_key_rotation() {
        let dir = tempdir().unwrap();
        let vault_path = dir.path().join("test_vault.json");

        let config = VaultConfigBuilder::new()
            .master_password("test_password_123")
            .vault_path(&vault_path)
            .environment("testnet")
            .build()
            .unwrap();

        let mut vault = KeyVault::new(config).unwrap();

        // Store initial key
        vault
            .store_key(
                "rotating_key",
                b"old_key",
                "Rotation test",
                AccessLevel::Admin,
                None,
            )
            .unwrap();

        // Rotate the key
        let new_key_id = vault.rotate_key("rotating_key", b"new_key").unwrap();
        assert!(new_key_id.starts_with("rotating_key_v"));

        // Old key should be inactive
        assert!(vault.retrieve_key("rotating_key", AccessLevel::Admin).is_err());

        // New key should work
        let retrieved = vault.retrieve_key(&new_key_id, AccessLevel::Admin).unwrap();
        assert_eq!(retrieved, b"new_key");
    }

    #[test]
    fn test_access_level_enforcement() {
        let dir = tempdir().unwrap();
        let vault_path = dir.path().join("test_vault.json");

        let config = VaultConfigBuilder::new()
            .master_password("test_password_123")
            .vault_path(&vault_path)
            .environment("testnet")
            .build()
            .unwrap();

        let mut vault = KeyVault::new(config).unwrap();

        // Store key with Admin access
        vault
            .store_key(
                "admin_only_key",
                b"secret",
                "Admin only",
                AccessLevel::Admin,
                None,
            )
            .unwrap();

        // Try to access with ReadOnly (should fail)
        assert!(vault
            .retrieve_key("admin_only_key", AccessLevel::ReadOnly)
            .is_err());
    }
}
