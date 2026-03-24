//! Shielded (Orchard) payment verification.
//!
//! Decrypts Orchard actions using an Incoming Viewing Key (IVK) and checks
//! that the payment amount and memo match expected values.
//!
//! Requires the `shielded` feature flag.

#![cfg(feature = "shielded")]

use std::fmt;

use orchard::keys::IncomingViewingKey;
use zcash_primitives::transaction::Transaction;
use zcash_protocol::consensus::BranchId;

use crate::replay::ConsumedTxids;
use crate::rpc::ZebradRpc;

const MEMO_PREFIX: &str = "zimppy:";

/// Request to verify a shielded Zcash payment.
#[derive(Debug, Clone)]
pub struct ShieldedVerifyRequest {
    pub txid: String,
    /// Orchard Incoming Viewing Key (serialized bytes, hex-encoded)
    pub ivk_bytes_hex: String,
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

/// Verify a shielded Orchard payment by decrypting actions with a viewing key.
///
/// The server holds an Orchard IVK which lets it see incoming payments
/// without being able to spend them. For each Orchard action in the tx,
/// we try to decrypt it. If decryption succeeds, the action was sent to us.
/// We then check the amount and memo match the expected values.
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

    // Parse transaction (Nu5 for Orchard support)
    let tx = Transaction::read(&raw_bytes[..], BranchId::Nu5).map_err(|e| {
        ShieldedVerifyError::ParseError(format!("failed to parse transaction: {e}"))
    })?;

    // Parse the IVK
    let ivk_bytes = hex::decode(&req.ivk_bytes_hex)
        .map_err(|e| ShieldedVerifyError::InvalidKey(e.to_string()))?;

    let ivk_array: [u8; 64] = ivk_bytes.try_into().map_err(|_| {
        ShieldedVerifyError::InvalidKey("Orchard IVK must be exactly 64 bytes".to_string())
    })?;

    let ivk = IncomingViewingKey::from_bytes(&ivk_array)
        .into_option()
        .ok_or_else(|| {
            ShieldedVerifyError::InvalidKey(
                "bytes do not represent a valid Orchard incoming viewing key".to_string(),
            )
        })?;

    // Get Orchard bundle
    let orchard_bundle =
        tx.orchard_bundle()
            .ok_or_else(|| ShieldedVerifyError::NoOrchardActions {
                txid: req.txid.clone(),
            })?;

    let action_count = orchard_bundle.actions().len();
    if action_count == 0 {
        return Err(ShieldedVerifyError::NoOrchardActions {
            txid: req.txid.clone(),
        });
    }

    let mut outputs_decrypted = 0u32;
    let mut best_amount: u64 = 0;
    let mut memo_matched = false;

    // Try to decrypt each Orchard action
    for idx in 0..action_count {
        if let Some((note, _address, memo)) = orchard_bundle.decrypt_output_with_key(idx, &ivk) {
            outputs_decrypted += 1;
            let value = note.value().inner();

            if value > best_amount {
                best_amount = value;
            }

            // Check memo for our challenge ID
            if let Ok(memo_str) = std::str::from_utf8(&memo) {
                let trimmed = memo_str.trim_end_matches('\0');
                if let Some(payload) = trimmed.strip_prefix(MEMO_PREFIX) {
                    if payload == req.expected_challenge_id {
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
    NoOrchardActions { txid: String },
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
            Self::NoOrchardActions { txid } => {
                write!(f, "transaction {txid} has no Orchard actions")
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
        let err = ShieldedVerifyError::NoOrchardActions {
            txid: "abc".to_string(),
        };
        assert!(err.to_string().contains("no Orchard actions"));
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
    fn rejects_invalid_ivk() {
        let err = ShieldedVerifyError::InvalidKey("bad key".to_string());
        assert!(err.to_string().contains("invalid viewing key"));
    }
}
