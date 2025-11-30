//! Cryptographic operations for secret storage

use crate::config::EncryptionConfig;
use anyhow::{anyhow, Result};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::pbkdf2;
use ring::rand::{SecureRandom, SystemRandom};
use std::num::NonZeroU32;
use zeroize::Zeroize;

/// Encrypted data with metadata
#[derive(Debug, Clone)]
pub struct EncryptedData {
    /// Salt used for key derivation
    pub salt: Vec<u8>,
    /// Nonce/IV
    pub nonce: Vec<u8>,
    /// Encrypted ciphertext with auth tag
    pub ciphertext: Vec<u8>,
}

impl EncryptedData {
    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = Vec::new();

        // Version byte
        data.push(1);

        // Salt length (1 byte) + salt
        data.push(self.salt.len() as u8);
        data.extend_from_slice(&self.salt);

        // Nonce length (1 byte) + nonce
        data.push(self.nonce.len() as u8);
        data.extend_from_slice(&self.nonce);

        // Ciphertext (rest of data)
        data.extend_from_slice(&self.ciphertext);

        data
    }

    /// Deserialize from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 4 {
            return Err(anyhow!("Invalid encrypted data: too short"));
        }

        let version = data[0];
        if version != 1 {
            return Err(anyhow!("Unsupported encryption version: {}", version));
        }

        let mut pos = 1;

        // Read salt
        let salt_len = data[pos] as usize;
        pos += 1;
        if pos + salt_len > data.len() {
            return Err(anyhow!("Invalid encrypted data: salt truncated"));
        }
        let salt = data[pos..pos + salt_len].to_vec();
        pos += salt_len;

        // Read nonce
        if pos >= data.len() {
            return Err(anyhow!("Invalid encrypted data: nonce missing"));
        }
        let nonce_len = data[pos] as usize;
        pos += 1;
        if pos + nonce_len > data.len() {
            return Err(anyhow!("Invalid encrypted data: nonce truncated"));
        }
        let nonce = data[pos..pos + nonce_len].to_vec();
        pos += nonce_len;

        // Rest is ciphertext
        let ciphertext = data[pos..].to_vec();

        Ok(Self {
            salt,
            nonce,
            ciphertext,
        })
    }
}

/// Cryptographic engine
pub struct CryptoEngine {
    config: EncryptionConfig,
    rng: SystemRandom,
}

impl CryptoEngine {
    /// Create new crypto engine
    pub fn new(config: EncryptionConfig) -> Self {
        Self {
            config,
            rng: SystemRandom::new(),
        }
    }

    /// Derive encryption key from password
    fn derive_key(&self, password: &[u8], salt: &[u8]) -> Result<[u8; 32]> {
        let iterations = NonZeroU32::new(self.config.pbkdf2_iterations)
            .ok_or_else(|| anyhow!("Invalid iteration count"))?;

        let mut key = [0u8; 32];
        pbkdf2::derive(
            pbkdf2::PBKDF2_HMAC_SHA256,
            iterations,
            salt,
            password,
            &mut key,
        );

        Ok(key)
    }

    /// Generate random bytes
    fn random_bytes(&self, len: usize) -> Result<Vec<u8>> {
        let mut bytes = vec![0u8; len];
        self.rng
            .fill(&mut bytes)
            .map_err(|_| anyhow!("Failed to generate random bytes"))?;
        Ok(bytes)
    }

    /// Encrypt data with password
    pub fn encrypt(&self, plaintext: &[u8], password: &[u8]) -> Result<EncryptedData> {
        // Generate salt and nonce
        let salt = self.random_bytes(self.config.salt_length)?;
        let nonce_bytes = self.random_bytes(12)?; // AES-GCM uses 12-byte nonce

        // Derive key
        let mut key_bytes = self.derive_key(password, &salt)?;

        // Create cipher
        let unbound_key = UnboundKey::new(&AES_256_GCM, &key_bytes)
            .map_err(|_| anyhow!("Failed to create encryption key"))?;
        let key = LessSafeKey::new(unbound_key);

        // Encrypt
        let nonce = Nonce::try_assume_unique_for_key(&nonce_bytes)
            .map_err(|_| anyhow!("Invalid nonce"))?;

        let mut ciphertext = plaintext.to_vec();
        key.seal_in_place_append_tag(nonce, Aad::empty(), &mut ciphertext)
            .map_err(|_| anyhow!("Encryption failed"))?;

        // Zero out key material
        key_bytes.zeroize();

        Ok(EncryptedData {
            salt,
            nonce: nonce_bytes,
            ciphertext,
        })
    }

    /// Decrypt data with password
    pub fn decrypt(&self, encrypted: &EncryptedData, password: &[u8]) -> Result<Vec<u8>> {
        // Derive key
        let mut key_bytes = self.derive_key(password, &encrypted.salt)?;

        // Create cipher
        let unbound_key = UnboundKey::new(&AES_256_GCM, &key_bytes)
            .map_err(|_| anyhow!("Failed to create decryption key"))?;
        let key = LessSafeKey::new(unbound_key);

        // Decrypt
        let nonce = Nonce::try_assume_unique_for_key(&encrypted.nonce)
            .map_err(|_| anyhow!("Invalid nonce"))?;

        let mut plaintext = encrypted.ciphertext.clone();
        let decrypted = key
            .open_in_place(nonce, Aad::empty(), &mut plaintext)
            .map_err(|_| anyhow!("Decryption failed - wrong password or corrupted data"))?;

        let result = decrypted.to_vec();

        // Zero out key material
        key_bytes.zeroize();

        Ok(result)
    }

    /// Verify password by attempting decryption
    pub fn verify_password(&self, encrypted: &EncryptedData, password: &[u8]) -> bool {
        self.decrypt(encrypted, password).is_ok()
    }

    /// Generate a random secret value
    pub fn generate_secret(&self, length: usize) -> Result<String> {
        let bytes = self.random_bytes(length)?;
        Ok(base64::encode(&bytes))
    }

    /// Generate a random password
    pub fn generate_password(&self, length: usize) -> Result<String> {
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*";

        let bytes = self.random_bytes(length)?;
        let password: String = bytes
            .iter()
            .map(|b| CHARSET[(*b as usize) % CHARSET.len()] as char)
            .collect();

        Ok(password)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let config = EncryptionConfig::default();
        let engine = CryptoEngine::new(config);

        let plaintext = b"Hello, Vault!";
        let password = b"test_password";

        let encrypted = engine.encrypt(plaintext, password).unwrap();
        let decrypted = engine.decrypt(&encrypted, password).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_wrong_password() {
        let config = EncryptionConfig::default();
        let engine = CryptoEngine::new(config);

        let plaintext = b"Hello, Vault!";
        let password = b"correct_password";
        let wrong_password = b"wrong_password";

        let encrypted = engine.encrypt(plaintext, password).unwrap();
        let result = engine.decrypt(&encrypted, wrong_password);

        assert!(result.is_err());
    }

    #[test]
    fn test_serialization() {
        let config = EncryptionConfig::default();
        let engine = CryptoEngine::new(config);

        let plaintext = b"Test data";
        let password = b"password";

        let encrypted = engine.encrypt(plaintext, password).unwrap();
        let bytes = encrypted.to_bytes();
        let restored = EncryptedData::from_bytes(&bytes).unwrap();

        let decrypted = engine.decrypt(&restored, password).unwrap();
        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }
}
