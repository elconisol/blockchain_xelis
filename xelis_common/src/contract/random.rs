// === External Crates ===
use anyhow::Context;
use blake3::OutputReader;
use serde::{Serialize, Deserialize};
use serde_json::Value;
use std::{
    sync::Arc,
    thread,
    time::Instant,
};
use xelis_vm::tid;

// === Local Imports ===
use crate::{
    account::CiphertextCache,
    asset::AssetData,
    block::TopoHeight,
    crypto::{Hash, PublicKey},
    wallet::{Wallet, WalletError},
    storage::ContractStorage,
    worker::{Worker, MinerWork, Algorithm, Difficulty},
    utils::{format_hashrate, get_current_time_in_millis},
};

// ======================================
// Thread Notifications & Socket Messages
// ======================================

#[derive(Clone)]
enum ThreadNotification<'a> {
    NewJob(Algorithm, MinerWork<'a>, Difficulty, u64), // POW algorithm, block work, difficulty, height
    WebSocketClosed, // WebSocket connection has been closed
    Exit,            // all threads must stop
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SocketMessage {
    NewJob(GetMinerWorkResult),
    BlockAccepted,
    BlockRejected(String),
}

// ========================
// Wallet-related functions
// ========================

impl Wallet {
    /// Decrypts an encrypted value loaded from disk.
    ///
    /// The encrypted data must contain:
    /// - a 24-byte nonce prefix,
    /// - followed by the ciphertext,
    /// - and optionally a salt prefix (which will be stripped after decryption).
    ///
    /// # Errors
    /// Returns a [`WalletError::InvalidEncryptedValue`] if the data is too short,
    /// or a [`WalletError::CryptoError`] if decryption fails.
    pub fn decrypt_value(&self, encrypted: &[u8]) -> anyhow::Result<Vec<u8>> {
        // Sanity check for minimum nonce size (24 bytes + 1 for ciphertext)
        if encrypted.len() < 25 {
            return Err(WalletError::InvalidEncryptedValue.into());
        }

        // Extract nonce (XChaCha20Poly1305 standard 24 bytes)
        let nonce = XNonce::from_slice(&encrypted[..24]);

        // Attempt decryption
        let mut decrypted = self
            .cipher
            .decrypt(nonce, &encrypted[24..])
            .map_err(WalletError::CryptoError)?;

        // Remove salt if present
        if let Some(salt) = &self.salt {
            decrypted.drain(..salt.len());
        }

        Ok(decrypted)
    }
}
