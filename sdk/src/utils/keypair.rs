use stellar_strkey::{ed25519, Strkey};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum KeyError {
    #[error("Invalid secret key format")]
    InvalidSecretKey,
    #[error("Failed to derive public key: {0}")]
    DerivationFailed(String),
}

/// Returns true if the given string is a valid Stellar public key (G...).
pub fn is_valid_public_key(key: &str) -> bool {
    Strkey::from_string(key)
        .map(|k| matches!(k, Strkey::PublicKeyEd25519(_)))
        .unwrap_or(false)
}

/// Returns true if the given string is a valid Stellar secret key (S...).
pub fn is_valid_secret_key(key: &str) -> bool {
    Strkey::from_string(key)
        .map(|k| matches!(k, Strkey::PrivateKeyEd25519(_)))
        .unwrap_or(false)
}

/// Derives the public key string from a Stellar secret key string.
pub fn public_key_from_secret(secret: &str) -> Result<String, KeyError> {
    let strkey = Strkey::from_string(secret).map_err(|_| KeyError::InvalidSecretKey)?;
    match strkey {
        Strkey::PrivateKeyEd25519(private) => {
            use ed25519_dalek::SigningKey;
            let signing_key = SigningKey::from_bytes(&private.0);
            let verifying_key = signing_key.verifying_key();
            let pub_strkey = ed25519::PublicKey(verifying_key.to_bytes());
            Ok(Strkey::PublicKeyEd25519(pub_strkey).to_string())
        }
        _ => Err(KeyError::InvalidSecretKey),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_PUBLIC: &str = "GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN";
    const VALID_SECRET: &str = "SCZANGBA5IPEKOCEQ7MPCQ3LUGO3JLGUJRXKELYPCLH2PFNKFK6GQOH";
    const INVALID: &str = "not-a-valid-key";

    #[test]
    fn test_valid_public_key() {
        assert!(is_valid_public_key(VALID_PUBLIC));
    }

    #[test]
    fn test_invalid_public_key() {
        assert!(!is_valid_public_key(INVALID));
        assert!(!is_valid_public_key(VALID_SECRET));
    }

    #[test]
    fn test_valid_secret_key() {
        assert!(is_valid_secret_key(VALID_SECRET));
    }

    #[test]
    fn test_invalid_secret_key() {
        assert!(!is_valid_secret_key(INVALID));
        assert!(!is_valid_secret_key(VALID_PUBLIC));
    }
}
