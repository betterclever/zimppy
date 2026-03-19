//! Shielded (Sapling) payment verification.
//!
//! Decrypts Sapling outputs using an Incoming Viewing Key (IVK) and checks
//! that the payment amount and memo match expected values.
//!
//! Requires the `shielded` feature flag.

#![cfg(feature = "shielded")]

use std::fmt;

use sapling_crypto::keys::{PreparedIncomingViewingKey, SaplingIvk};
use sapling_crypto::note_encryption::{try_sapling_note_decryption, Zip212Enforcement};
use zcash_primitives::transaction::Transaction;
use zcash_protocol::consensus::BranchId;

use crate::replay::ConsumedTxids;
use crate::rpc::ZebradRpc;

const MEMO_PREFIX: &str = "zimppy:";

/// Request to verify a shielded Zcash payment.
#[derive(Debug, Clone)]
pub struct ShieldedVerifyRequest {
    pub txid: String,
    /// Sapling Incoming Viewing Key (raw 32 bytes, hex-encoded)
    pub ivk_hex: String,
    /// Expected challenge ID in the memo field
    pub expected_challenge_id: String,
    /// Expected payment amount in zatoshis
    pub expected_amount_zat: u64,
}

/// Result of a shielded payment verification.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ShieldedVerifyResult {
    pub verified: bool,
    pub txid: String,
    pub observed_amount_zat: u64,
    pub memo_matched: bool,
    pub outputs_decrypted: u32,
}

/// Verify a shielded Sapling payment by decrypting outputs with a viewing key.
pub async fn verify_shielded(
    rpc: &ZebradRpc,
    req: &ShieldedVerifyRequest,
    consumed: &ConsumedTxids,
) -> Result<ShieldedVerifyResult, ShieldedVerifyError> {
    // Replay protection
    consumed
        .check_and_insert(&req.txid)
        .map_err(|_| ShieldedVerifyError::ReplayDetected {
            txid: req.txid.clone(),
        })?;

    // Fetch raw transaction hex
    let raw_hex = rpc
        .get_raw_transaction_hex(&req.txid)
        .await
        .map_err(|e| ShieldedVerifyError::Rpc(e.to_string()))?;

    let raw_bytes =
        hex::decode(&raw_hex).map_err(|e| ShieldedVerifyError::ParseError(e.to_string()))?;

    // Parse transaction (try Nu5 branch first, as testnet uses v5 txs)
    let tx = Transaction::read(&raw_bytes[..], BranchId::Nu5)
        .map_err(|e| ShieldedVerifyError::ParseError(format!("failed to parse transaction: {e}")))?;

    // Parse the IVK (raw 32 bytes → jubjub::Fr → SaplingIvk → PreparedIncomingViewingKey)
    let ivk_bytes =
        hex::decode(&req.ivk_hex).map_err(|e| ShieldedVerifyError::InvalidKey(e.to_string()))?;

    if ivk_bytes.len() != 32 {
        return Err(ShieldedVerifyError::InvalidKey(format!(
            "IVK must be 32 bytes, got {}",
            ivk_bytes.len()
        )));
    }

    let ivk_array: [u8; 32] = ivk_bytes
        .try_into()
        .map_err(|_| ShieldedVerifyError::InvalidKey("IVK must be exactly 32 bytes".to_string()))?;

    let fr = jubjub::Fr::from_bytes(&ivk_array);
    if fr.is_none().into() {
        return Err(ShieldedVerifyError::InvalidKey(
            "IVK bytes do not represent a valid jubjub scalar".to_string(),
        ));
    }
    let ivk = SaplingIvk(fr.unwrap());
    let prepared_ivk = PreparedIncomingViewingKey::new(&ivk);

    // Get Sapling bundle
    let sapling_bundle = tx
        .sapling_bundle()
        .ok_or_else(|| ShieldedVerifyError::NoSaplingOutputs {
            txid: req.txid.clone(),
        })?;

    let outputs = sapling_bundle.shielded_outputs();
    if outputs.is_empty() {
        return Err(ShieldedVerifyError::NoSaplingOutputs {
            txid: req.txid.clone(),
        });
    }

    let mut outputs_decrypted = 0u32;
    let mut best_amount: u64 = 0;
    let mut memo_matched = false;

    // Try to decrypt each Sapling output using the sapling-specific helper
    for output in outputs {
        if let Some((note, _recipient, memo)) =
            try_sapling_note_decryption(&prepared_ivk, output, Zip212Enforcement::On)
        {
            outputs_decrypted += 1;
            let value = note.value().inner();

            if value > best_amount {
                best_amount = value;
            }

            // Check memo for our challenge ID — D::Memo is [u8; 512]
            if let Ok(memo_str) = std::str::from_utf8(&memo) {
                let trimmed = memo_str.trim_end_matches('\0');
                if let Some(payload) = trimmed.strip_prefix(MEMO_PREFIX) {
                    if payload.contains(&req.expected_challenge_id) {
                        memo_matched = true;
                    }
                }
            }
        }
    }

    let amount_sufficient = best_amount >= req.expected_amount_zat;
    let verified = outputs_decrypted > 0 && amount_sufficient && memo_matched;

    if !verified {
        consumed.remove(&req.txid);
    }

    Ok(ShieldedVerifyResult {
        verified,
        txid: req.txid.clone(),
        observed_amount_zat: best_amount,
        memo_matched,
        outputs_decrypted,
    })
}

#[derive(Debug, Clone)]
pub enum ShieldedVerifyError {
    Rpc(String),
    ReplayDetected { txid: String },
    ParseError(String),
    InvalidKey(String),
    NoSaplingOutputs { txid: String },
}

impl fmt::Display for ShieldedVerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rpc(e) => write!(f, "rpc error: {e}"),
            Self::ReplayDetected { txid } => {
                write!(f, "replay detected: txid {txid} already consumed")
            }
            Self::ParseError(e) => write!(f, "parse error: {e}"),
            Self::InvalidKey(e) => write!(f, "invalid viewing key: {e}"),
            Self::NoSaplingOutputs { txid } => {
                write!(f, "transaction {txid} has no Sapling outputs")
            }
        }
    }
}

impl std::error::Error for ShieldedVerifyError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shielded_verify_error_displays() {
        let err = ShieldedVerifyError::NoSaplingOutputs {
            txid: "abc".to_string(),
        };
        assert!(err.to_string().contains("no Sapling outputs"));
    }

    #[test]
    fn shielded_verify_result_serializes() {
        let result = ShieldedVerifyResult {
            verified: true,
            txid: "abc".to_string(),
            observed_amount_zat: 42000,
            memo_matched: true,
            outputs_decrypted: 1,
        };
        let json = serde_json::to_string(&result).expect("should serialize");
        assert!(json.contains("\"verified\":true"));
        assert!(json.contains("\"memo_matched\":true"));
    }

    #[test]
    fn rejects_invalid_ivk_length() {
        let err = ShieldedVerifyError::InvalidKey("IVK must be 32 bytes, got 16".to_string());
        assert!(err.to_string().contains("32 bytes"));
    }
}
