use anyhow::Result;
use chacha20poly1305::{
    aead::Aead,
    XNonce,
    aead::OsRng,
    XChaCha20Poly1305,
    AeadCore,
    KeyInit
};
use xelis_common::crypto::{
    HASH_SIZE,
    hash
};
use crate::{error::WalletError, config::SALT_SIZE};


pub struct Cipher {
    cipher: XChaCha20Poly1305,
    // this salt is used for keys and values
    salt: Option<[u8; SALT_SIZE]>
}

impl Cipher {
    pub const NONCE_SIZE: usize = 24;

    pub fn new(key: &[u8], salt: Option<[u8; SALT_SIZE]>) -> Result<Self> {
        Ok(Self {
            cipher: XChaCha20Poly1305::new_from_slice(key)?,
            salt
        })
    }

    // encrypt value passed in param and add plaintext nonce before encrypted value
    // a Nonce is generated randomly at each call
    pub fn encrypt_value(&self, value: &[u8]) -> Result<Vec<u8>, WalletError> {
        // generate unique random nonce
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        self.encrypt_value_with_nonce(value, &nonce.into())
    }

    // encrypt value passed in param and add plaintext nonce before encrypted value
    pub fn encrypt_value_with_nonce(&self, value: &[u8], nonce: &[u8; Self::NONCE_SIZE]) -> Result<Vec<u8>, WalletError> {
        let mut plaintext: Vec<u8> = Vec::with_capacity(SALT_SIZE + value.len());
        // add salt to the plaintext value
        if let Some(salt) = &self.salt {
            plaintext.extend_from_slice(salt);
        }
        plaintext.extend_from_slice(value);

        // encrypt data using plaintext and nonce
        let data = &self.cipher.encrypt(nonce.into(), plaintext.as_slice()).map_err(|e| WalletError::CryptoError(e))?;

        // append unique nonce to the encrypted data
        let mut encrypted = Vec::with_capacity(Self::NONCE_SIZE + data.len());
        encrypted.extend_from_slice(nonce);
        encrypted.extend_from_slice(data);

        Ok(encrypted)
    }

  /// Decrypts a value previously encrypted with `encrypt_value`.
/// Expects format: [24-byte nonce || ciphertext].
pub fn decrypt_value(&self, encrypted: &[u8]) -> Result<Vec<u8>> {
    // Ensure slice contains at least nonce + 1 byte
    if encrypted.len() <= 24 {
        return Err(WalletError::InvalidEncryptedValue.into());
    }

    // Extract nonce
    let nonce = XNonce::from_slice(&encrypted[..24]);

    // Decrypt remaining slice
    let mut decrypted = self
        .cipher
        .decrypt(nonce, &encrypted[24..])
        .map_err(WalletError::CryptoError)?;

    // Remove optional salt prefix
    if let Some(salt) = &self.salt {
        if decrypted.len() < salt.len() {
            return Err(WalletError::InvalidEncryptedValue.into());
        }
        decrypted.drain(..salt.len());
    }

    Ok(decrypted)
}

    // hash the key with salt
    pub fn hash_key<S: AsRef<[u8]>>(&self, key: S) -> [u8; HASH_SIZE] {
        let mut data = Vec::new();
        if let Some(salt) = &self.salt {
            data.extend_from_slice(salt);
        }
        data.extend_from_slice(key.as_ref());
        hash(&data).to_bytes()
    }
}
