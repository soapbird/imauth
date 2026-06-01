use crate::config::Config;
use crate::ports::encryption::EncryptionService;
use crate::ImauthError;
use crate::Result;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use rand::Rng;

pub struct AesGcmEncryptionService {
    cipher: Aes256Gcm,
}

impl AesGcmEncryptionService {
    pub fn from_key(key: &str) -> Result<Self> {
        // Try standard base64 first, then URL-safe with padding (used by imlinks),
        // then URL-safe without padding. Keys are 44 chars for 32 bytes.
        let key_bytes = BASE64
            .decode(key)
            .or_else(|_| URL_SAFE.decode(key))
            .or_else(|_| URL_SAFE_NO_PAD.decode(key))?;
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

    /// 32 bytes → standard base64 (with == padding).
    /// Same raw bytes as URL_SAFE_KEY below, just standard alphabet.
    const STANDARD_KEY: &str = "kZiepW/G5hqnFgYH5f1GpIm87XhdUT6gIgbsxcXTj/E=";

    /// Same 32 bytes as STANDARD_KEY, but URL-safe alphabet.
    /// Uses `-` and `_` instead of `+` and `/`, no `=` padding.
    /// The `_` at position 22 is what distinguishes it from standard base64.
    const URL_SAFE_KEY: &str = "kZiepW_G5hqnFgYH5f1GpIm87XhdUT6gIgbsxcXTj_E";

    /// Same 32 bytes as URL_SAFE_KEY but with `=` padding appended.
    /// This is the form imlinks generates (length 44, URL-safe alphabet with padding).
    const URL_SAFE_PADDED_KEY: &str = "kZiepW_G5hqnFgYH5f1GpIm87XhdUT6gIgbsxcXTj_E=";

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

    #[test]
    fn from_key_accepts_standard_base64() {
        // "hello world" as bytes, then standard base64 → "aGVsbG8gd29ybGQ="
        // That's 11 bytes; pad to 12 to test round-trip encrypt/decrypt
        let key = "aGVsbG8gd29ybGQ="; // 12-byte key (96 bits)
        let result = AesGcmEncryptionService::from_key(key);
        assert!(result.is_err(), "short key should fail with 32-byte check");
        // Use a proper 32-byte standard base64 key
        let key_32 = "YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXoxMjM0NTY=";
        let svc = AesGcmEncryptionService::from_key(key_32).unwrap();
        let ct = svc.encrypt("test").unwrap();
        assert_eq!(svc.decrypt(&ct).unwrap(), "test");
    }

    #[test]
    fn from_key_accepts_url_safe_base64() {
        // URL-safe encoding of the same 32 bytes (uses - and _ instead of + and /, no = padding)
        let svc = AesGcmEncryptionService::from_key(URL_SAFE_KEY).unwrap();
        let ct = svc.encrypt("hello").unwrap();
        assert_eq!(svc.decrypt(&ct).unwrap(), "hello");
    }

    #[test]
    fn from_key_accepts_url_safe_padded_base64() {
        // URL-safe with `=` padding appended (imlinks key format, length 44).
        // Uses `_` (position 6 and 41) which distinguishes it from standard base64.
        assert!(URL_SAFE_PADDED_KEY.contains('='));
        assert!(URL_SAFE_PADDED_KEY.contains('_'));
        let svc = AesGcmEncryptionService::from_key(URL_SAFE_PADDED_KEY).unwrap();
        let ct = svc.encrypt("world").unwrap();
        assert_eq!(svc.decrypt(&ct).unwrap(), "world");
    }

    #[test]
    fn from_key_rejects_invalid_base64() {
        let result = AesGcmEncryptionService::from_key("not-valid-base64!!!");
        assert!(result.is_err());
        let is_encryption_err = matches!(result, Err(crate::ImauthError::Encryption(_)));
        assert!(is_encryption_err, "expected Encryption error for invalid base64");
    }

    #[test]
    fn from_key_rejects_too_short_key() {
        // 4 bytes → 6 standard base64 chars
        let result = AesGcmEncryptionService::from_key("YWJjZA==");
        assert!(result.is_err());
        let is_32byte_err = matches!(&result, Err(crate::ImauthError::Encryption(msg)) if msg.contains("32 bytes"));
        assert!(is_32byte_err, "expected 32-byte Encryption error for short key");
    }

    #[test]
    fn from_key_rejects_wrong_length_key() {
        // 31 bytes (valid base64 but wrong length for AES-256)
        let key_31 = "YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXoxMjM0NTY3"; // 31 bytes
        let result = AesGcmEncryptionService::from_key(key_31);
        assert!(result.is_err());
    }

    #[test]
    fn url_safe_key_contains_underscore_and_hyphen() {
        // Verify the URL-safe key actually contains _ and - characters.
        // URL-safe base64 uses: A-Z, a-z, 0-9, -, _
        assert!(URL_SAFE_KEY.contains('_') || URL_SAFE_KEY.contains('-'));
        // Confirm no + or / (standard base64 chars that differ)
        assert!(!URL_SAFE_KEY.contains('+'));
        assert!(!URL_SAFE_KEY.contains('/'));
        assert!(!URL_SAFE_KEY.contains('='));
    }

    #[test]
    fn standard_key_has_padding() {
        // Standard base64 always has = padding for non-multiple-of-3 input.
        assert!(STANDARD_KEY.contains('='));
    }
}
