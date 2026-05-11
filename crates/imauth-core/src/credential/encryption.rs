use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rand::Rng;

pub struct AesGcmEncryption {
    cipher: Aes256Gcm,
}

impl AesGcmEncryption {
    pub fn from_key(key: &str) -> crate::Result<Self> {
        let key_bytes = BASE64
            .decode(key)
            .map_err(|_| crate::ImauthError::Encryption("Invalid base64 key".to_string()))?;
        if key_bytes.len() != 32 {
            return Err(crate::ImauthError::Encryption(
                "Key must be 32 bytes (256 bits)".to_string(),
            ));
        }
        let cipher = Aes256Gcm::new_from_slice(&key_bytes)
            .map_err(|e| crate::ImauthError::Encryption(e.to_string()))?;
        Ok(Self { cipher })
    }

    pub fn generate_key() -> String {
        let mut key = [0u8; 32];
        rand::thread_rng().fill(&mut key);
        BASE64.encode(key)
    }

    pub fn encrypt(&self, plaintext: &str) -> crate::Result<String> {
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| crate::ImauthError::Encryption(e.to_string()))?;
        let mut result = nonce_bytes.to_vec();
        result.extend_from_slice(&ciphertext);
        Ok(BASE64.encode(&result))
    }

    pub fn decrypt(&self, ciphertext: &str) -> crate::Result<String> {
        let data = BASE64
            .decode(ciphertext)
            .map_err(|_| crate::ImauthError::Encryption("Invalid base64 ciphertext".to_string()))?;
        if data.len() < 12 {
            return Err(crate::ImauthError::Encryption(
                "Ciphertext too short".to_string(),
            ));
        }
        let (nonce_bytes, encrypted) = data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = self
            .cipher
            .decrypt(nonce, encrypted)
            .map_err(|e| crate::ImauthError::Encryption(e.to_string()))?;
        String::from_utf8(plaintext)
            .map_err(|e| crate::ImauthError::Encryption(format!("Invalid UTF-8: {e}")))
    }
}

/// Decrypt a Fernet-encrypted value (AES-128-CBC with HMAC, Python cryptography.fernet).
/// This is a compatibility shim for migrating from Python Fernet.
pub struct FernetCompat;

impl FernetCompat {
    pub fn decrypt(_ciphertext: &str, _key: &str) -> crate::Result<String> {
        // Fernet uses AES-128-CBC + HMAC-SHA256. Implementing full Fernet in Rust
        // requires the `fernet` crate. For now, this is a placeholder that returns
        // an error directing the user to migrate or use the fernet crate.
        Err(crate::ImauthError::Encryption(
            "Fernet decryption not implemented. Use fernet crate or migrate credentials."
                .to_string(),
        ))
    }
}
