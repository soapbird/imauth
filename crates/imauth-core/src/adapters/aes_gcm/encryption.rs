use crate::config::Config;
use crate::ports::encryption::EncryptionService;
use crate::ImauthError;
use crate::Result;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rand::Rng;

pub struct AesGcmEncryptionService {
    cipher: Aes256Gcm,
}

impl AesGcmEncryptionService {
    pub fn from_key(key: &str) -> Result<Self> {
        let key_bytes = BASE64.decode(key)?;
        if key_bytes.len() != 32 {
            return Err(ImauthError::Encryption(
                "Key must be 32 bytes (256 bits)".to_string(),
            ));
        }
        let cipher = Aes256Gcm::new_from_slice(&key_bytes)
            .map_err(|e| ImauthError::Encryption(e.to_string()))?;
        Ok(Self { cipher })
    }

    /// Build the encryption service from a config. A key must be configured;
    /// otherwise an error is returned.
    pub fn from_config(config: &Config) -> Result<Self> {
        let key = config.encryption_key().ok_or_else(|| {
            ImauthError::Config(
                "Encryption key is required. Set IMAUTH_ENCRYPTION_KEY environment variable \
                     or add encryption_key to [security] in config.toml. \
                     Generate a key with: openssl rand -base64 32"
                    .to_string(),
            )
        })?;
        Self::from_key(key)
    }
}

pub fn generate_key() -> String {
    let mut key = [0u8; 32];
    rand::thread_rng().fill(&mut key);
    BASE64.encode(key)
}

impl EncryptionService for AesGcmEncryptionService {
    fn encrypt(&self, plaintext: &str) -> Result<String> {
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self.cipher.encrypt(nonce, plaintext.as_bytes())?;
        let mut result = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);
        Ok(BASE64.encode(&result))
    }

    fn decrypt(&self, ciphertext: &str) -> Result<String> {
        let data = BASE64.decode(ciphertext)?;
        if data.len() < 12 {
            return Err(ImauthError::Encryption("Ciphertext too short".to_string()));
        }
        let (nonce_bytes, encrypted) = data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = self.cipher.decrypt(nonce, encrypted)?;
        String::from_utf8(plaintext)
            .map_err(|e| ImauthError::Encryption(format!("Invalid UTF-8: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn from_config_rejects_missing_key() {
        let config = Config::default();
        let result = AesGcmEncryptionService::from_config(&config);
        assert!(result.is_err());
        match result {
            Err(crate::ImauthError::Config(msg)) => {
                assert!(
                    msg.contains("Encryption key is required"),
                    "expected key-required error, got: {msg}"
                );
            }
            _ => panic!("Expected Config error"),
        }
    }

    #[test]
    fn from_config_accepts_configured_key() {
        let mut config = Config::default();
        config.security.encryption_key = Some(generate_key());
        assert!(AesGcmEncryptionService::from_config(&config).is_ok());
    }
}
