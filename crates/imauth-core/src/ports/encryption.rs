use crate::Result;

/// Symmetric encryption boundary used by credential storage. Sync because
/// the underlying AEAD primitive is CPU-bound and a swap to KMS will go
/// through a blocking pool, not an async API.
pub trait EncryptionService: Send + Sync {
    fn encrypt(&self, plaintext: &str) -> Result<String>;
    fn decrypt(&self, ciphertext: &str) -> Result<String>;
}
