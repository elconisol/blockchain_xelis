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
    // Decrypt value loaded from disk
    pub fn decrypt_value(&self, encrypted: &[u8]) -> anyhow::Result<Vec<u8>> {
        if encrypted.len() < 25 {
            return Err(WalletError::InvalidEncryptedValue.into());
        }

        let nonce = XNonce::from_slice(&encrypted[0..24]);
        let mut decrypted = self.cipher
            .decrypt(nonce, &encrypted[nonce.len()..])
            .map_err(WalletError::CryptoError)?;

        if let Some(salt) = &self.salt {
            decrypted.drain(0..salt.len());
        }

        Ok(decrypted)
    }
}
