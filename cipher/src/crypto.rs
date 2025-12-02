//! Cryptographic operations

use argon2::{Argon2, PasswordHasher, PasswordHash, PasswordVerifier};
use argon2::password_hash::SaltString;
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use rand::RngCore;
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Cryptographic operation errors
#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("key derivation failed: {0}")]
    KeyDerivation(String),

    #[error("invalid key length: expected {expected}, got {actual}")]
    InvalidKeyLength { expected: usize, actual: usize },

    #[error("encryption failed: {0}")]
    Encryption(String),

    #[error("decryption failed: {reason}")]
    Decryption { reason: &'static str },

    #[error("ciphertext too short: need at least {min_bytes} bytes for nonce")]
    CiphertextTooShort { min_bytes: usize },

    #[error("password hashing failed: {0}")]
    PasswordHash(String),

    #[error("invalid hash format: {0}")]
    InvalidHashFormat(String),

    #[error("invalid UTF-8 in secret")]
    InvalidUtf8(#[from] std::str::Utf8Error),
}

type Result<T> = std::result::Result<T, CryptoError>;

/// Encryption key (zeroized on drop)
#[derive(Clone, ZeroizeOnDrop)]
pub struct EncryptionKey {
    #[zeroize(skip)]
    key: [u8; 32],
}

impl EncryptionKey {
    /// Generate a random key
    pub fn generate() -> Self {
        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        Self { key }
    }

    /// Derive key from password
    pub fn derive_from_password(password: &str, salt: &[u8]) -> Result<Self> {
        let argon2 = Argon2::default();

        let mut key = [0u8; 32];
        argon2.hash_password_into(
            password.as_bytes(),
            salt,
            &mut key,
        ).map_err(|e| CryptoError::KeyDerivation(e.to_string()))?;

        Ok(Self { key })
    }

    /// Encrypt data
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let cipher = ChaCha20Poly1305::new_from_slice(&self.key)
            .map_err(|_| CryptoError::InvalidKeyLength { expected: 32, actual: self.key.len() })?;

        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher.encrypt(nonce, plaintext)
            .map_err(|e| CryptoError::Encryption(e.to_string()))?;

        // Prepend nonce to ciphertext
        let mut result = nonce_bytes.to_vec();
        result.extend(ciphertext);

        Ok(result)
    }

    /// Decrypt data
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        if ciphertext.len() < 12 {
            return Err(CryptoError::CiphertextTooShort { min_bytes: 12 });
        }

        let cipher = ChaCha20Poly1305::new_from_slice(&self.key)
            .map_err(|_| CryptoError::InvalidKeyLength { expected: 32, actual: self.key.len() })?;

        let nonce = Nonce::from_slice(&ciphertext[..12]);
        let data = &ciphertext[12..];

        let plaintext = cipher.decrypt(nonce, data)
            .map_err(|_| CryptoError::Decryption { reason: "authentication failed or corrupted data" })?;

        Ok(plaintext)
    }
}

/// Generate a random salt
pub fn generate_salt() -> [u8; 16] {
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    salt
}

/// Hash a password for storage
pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut rand::thread_rng());
    let argon2 = Argon2::default();

    let hash = argon2.hash_password(password.as_bytes(), &salt)
        .map_err(|e| CryptoError::PasswordHash(e.to_string()))?;

    Ok(hash.to_string())
}

/// Verify a password against stored hash
pub fn verify_password(password: &str, hash: &str) -> Result<bool> {
    let parsed_hash = PasswordHash::new(hash)
        .map_err(|e| CryptoError::InvalidHashFormat(e.to_string()))?;

    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

/// Secure secret that zeroizes on drop
#[derive(Clone, ZeroizeOnDrop)]
pub struct Secret {
    data: Vec<u8>,
}

impl Secret {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    pub fn from_str(s: &str) -> Self {
        Self { data: s.as_bytes().to_vec() }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    pub fn as_str(&self) -> Result<&str> {
        Ok(std::str::from_utf8(&self.data)?)
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Secret([REDACTED])")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_roundtrip() {
        let key = EncryptionKey::generate();
        let plaintext = b"Hello, Nyx!";

        let ciphertext = key.encrypt(plaintext).expect("encryption should succeed");
        let decrypted = key.decrypt(&ciphertext).expect("decryption should succeed");

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_encryption_produces_different_ciphertext() {
        let key = EncryptionKey::generate();
        let plaintext = b"Hello, Nyx!";

        let ct1 = key.encrypt(plaintext).unwrap();
        let ct2 = key.encrypt(plaintext).unwrap();

        // Different nonces should produce different ciphertext
        assert_ne!(ct1, ct2);
    }

    #[test]
    fn test_decrypt_short_ciphertext_fails() {
        let key = EncryptionKey::generate();
        let short_data = vec![0u8; 8]; // Less than 12 bytes

        let result = key.decrypt(&short_data);
        assert!(matches!(result, Err(CryptoError::CiphertextTooShort { min_bytes: 12 })));
    }

    #[test]
    fn test_decrypt_corrupted_data_fails() {
        let key = EncryptionKey::generate();
        let plaintext = b"Hello, Nyx!";

        let mut ciphertext = key.encrypt(plaintext).unwrap();
        // Corrupt the ciphertext
        ciphertext[15] ^= 0xFF;

        let result = key.decrypt(&ciphertext);
        assert!(matches!(result, Err(CryptoError::Decryption { .. })));
    }

    #[test]
    fn test_password_hash_verify() {
        let password = "secure_password_123";
        let hash = hash_password(password).expect("hashing should succeed");

        assert!(verify_password(password, &hash).expect("verification should succeed"));
        assert!(!verify_password("wrong_password", &hash).expect("verification should succeed"));
    }

    #[test]
    fn test_key_derivation() {
        let password = "my_secret_password";
        let salt = generate_salt();

        let key1 = EncryptionKey::derive_from_password(password, &salt)
            .expect("derivation should succeed");
        let key2 = EncryptionKey::derive_from_password(password, &salt)
            .expect("derivation should succeed");

        // Same password + salt should produce same key
        assert_eq!(key1.key, key2.key);
    }

    #[test]
    fn test_secret_redacts_debug() {
        let secret = Secret::from_str("super_secret");
        let debug_str = format!("{:?}", secret);

        assert!(!debug_str.contains("super_secret"));
        assert!(debug_str.contains("REDACTED"));
    }

    #[test]
    fn test_secret_as_str() {
        let secret = Secret::from_str("hello");
        assert_eq!(secret.as_str().unwrap(), "hello");
    }

    #[test]
    fn test_secret_invalid_utf8() {
        let secret = Secret::new(vec![0xFF, 0xFE]); // Invalid UTF-8
        assert!(matches!(secret.as_str(), Err(CryptoError::InvalidUtf8(_))));
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Any plaintext can be encrypted and decrypted back to the original
        #[test]
        fn prop_encrypt_decrypt_roundtrip(plaintext in prop::collection::vec(any::<u8>(), 0..1024)) {
            let key = EncryptionKey::generate();
            let ciphertext = key.encrypt(&plaintext)?;
            let decrypted = key.decrypt(&ciphertext)?;
            prop_assert_eq!(plaintext, decrypted);
        }

        /// Encryption with different keys produces different ciphertext
        #[test]
        fn prop_different_keys_different_ciphertext(plaintext in prop::collection::vec(any::<u8>(), 1..256)) {
            let key1 = EncryptionKey::generate();
            let key2 = EncryptionKey::generate();

            let ct1 = key1.encrypt(&plaintext)?;
            let ct2 = key2.encrypt(&plaintext)?;

            // Ciphertext should differ (extremely high probability)
            prop_assert_ne!(ct1, ct2);
        }

        /// Key derivation is deterministic for same password+salt
        #[test]
        fn prop_key_derivation_deterministic(
            password in "[a-zA-Z0-9]{8,32}",
            salt in prop::collection::vec(any::<u8>(), 16..17)
        ) {
            let key1 = EncryptionKey::derive_from_password(&password, &salt)?;
            let key2 = EncryptionKey::derive_from_password(&password, &salt)?;
            prop_assert_eq!(key1.key, key2.key);
        }

        /// Password verification succeeds for correct password
        #[test]
        fn prop_password_verify_correct(password in "[a-zA-Z0-9!@#$%^&*]{8,64}") {
            let hash = hash_password(&password)?;
            prop_assert!(verify_password(&password, &hash)?);
        }

        /// Password verification fails for wrong password
        #[test]
        fn prop_password_verify_wrong(
            password1 in "[a-zA-Z0-9]{8,32}",
            password2 in "[a-zA-Z0-9]{8,32}"
        ) {
            prop_assume!(password1 != password2);
            let hash = hash_password(&password1)?;
            prop_assert!(!verify_password(&password2, &hash)?);
        }
    }
}
