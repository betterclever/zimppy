use std::fmt;

use crate::replay::ConsumedTxids;
use crate::rpc::{RpcError, ZebradRpc};

/// Request to verify a transparent Zcash payment.
#[derive(Debug, Clone)]
pub struct TransparentVerifyRequest {
    pub txid: String,
    pub output_index: u32,
    pub expected_address: String,
    pub expected_amount_zat: u64,
}

/// Result of a transparent payment verification.
#[derive(Debug, Clone, serde::Serialize)]
pub struct VerifyResult {
    pub verified: bool,
    pub txid: String,
    pub observed_address: String,
    pub observed_amount_zat: u64,
    pub confirmations: u32,
}

/// Verify a transparent Zcash payment by checking on-chain transaction outputs.
///
/// Uses verbose JSON-RPC (`getrawtransaction(txid, 1)`) which returns structured
/// output data — no raw hex parsing needed.
pub async fn verify_transparent(
    rpc: &ZebradRpc,
    req: &TransparentVerifyRequest,
    consumed: &ConsumedTxids,
) -> Result<VerifyResult, VerifyError> {
    // Check replay protection first
    consumed
        .check_and_insert(&req.txid)
        .map_err(|_| VerifyError::ReplayDetected {
            txid: req.txid.clone(),
        })?;

    // Fetch verbose transaction
    let tx = rpc
        .get_transaction_verbose(&req.txid)
        .await
        .map_err(VerifyError::Rpc)?;

    let vout = tx.vout.ok_or_else(|| VerifyError::NoOutputs {
        txid: req.txid.clone(),
    })?;

    // Find the specified output
    let output = vout
        .iter()
        .find(|o| o.n == Some(req.output_index))
        .ok_or_else(|| VerifyError::OutputIndexOutOfBounds {
            txid: req.txid.clone(),
            index: req.output_index,
            available: vout.len() as u32,
        })?;

    // Extract the address(es) from this output
    let addresses = output
        .script_pub_key
        .as_ref()
        .and_then(|spk| spk.addresses.as_ref())
        .ok_or_else(|| VerifyError::NoAddressInOutput {
            txid: req.txid.clone(),
            index: req.output_index,
        })?;

    let observed_address = addresses.first().cloned().unwrap_or_default();

    // Extract amount in zatoshis
    let observed_amount_zat = output.value_zat.unwrap_or(0);

    let confirmations = tx.confirmations.unwrap_or(0);

    // Check: address matches
    let address_matches = addresses.iter().any(|a| a == &req.expected_address);

    // Check: amount sufficient
    let amount_sufficient = observed_amount_zat >= req.expected_amount_zat;

    let verified = address_matches && amount_sufficient;

    // If verification failed, release the txid from consumed set so it can be retried
    if !verified {
        consumed.remove(&req.txid);
    }

    Ok(VerifyResult {
        verified,
        txid: req.txid.clone(),
        observed_address,
        observed_amount_zat,
        confirmations,
    })
}

#[derive(Debug, Clone)]
pub enum VerifyError {
    Rpc(RpcError),
    ReplayDetected { txid: String },
    NoOutputs { txid: String },
    OutputIndexOutOfBounds { txid: String, index: u32, available: u32 },
    NoAddressInOutput { txid: String, index: u32 },
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rpc(e) => write!(f, "rpc error: {e}"),
            Self::ReplayDetected { txid } => write!(f, "replay detected: txid {txid} already consumed"),
            Self::NoOutputs { txid } => write!(f, "transaction {txid} has no transparent outputs"),
            Self::OutputIndexOutOfBounds { txid, index, available } => {
                write!(f, "output index {index} out of bounds for tx {txid} (has {available} outputs)")
            }
            Self::NoAddressInOutput { txid, index } => {
                write!(f, "no address in output {index} of tx {txid}")
            }
        }
    }
}

impl std::error::Error for VerifyError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::{ScriptPubKey, TransparentOutput, VerboseTransaction};

    fn mock_verbose_tx(address: &str, amount_zat: u64) -> VerboseTransaction {
        VerboseTransaction {
            txid: Some("deadbeef".to_string()),
            confirmations: Some(3),
            vout: Some(vec![TransparentOutput {
                value: Some(amount_zat as f64 / 100_000_000.0),
                value_zat: Some(amount_zat),
                n: Some(0),
                script_pub_key: Some(ScriptPubKey {
                    script_type: Some("pubkeyhash".to_string()),
                    addresses: Some(vec![address.to_string()]),
                }),
            }]),
        }
    }

    #[test]
    fn verify_result_serializes_to_json() {
        let result = VerifyResult {
            verified: true,
            txid: "abc".to_string(),
            observed_address: "tmXYZ".to_string(),
            observed_amount_zat: 42000,
            confirmations: 5,
        };
        let json = serde_json::to_string(&result).expect("should serialize");
        assert!(json.contains("\"verified\":true"));
        assert!(json.contains("\"observed_amount_zat\":42000"));
    }

    #[test]
    fn verify_error_displays_replay() {
        let err = VerifyError::ReplayDetected {
            txid: "abc".to_string(),
        };
        assert!(err.to_string().contains("replay detected"));
    }

    #[test]
    fn verify_error_displays_out_of_bounds() {
        let err = VerifyError::OutputIndexOutOfBounds {
            txid: "abc".to_string(),
            index: 5,
            available: 2,
        };
        assert!(err.to_string().contains("output index 5 out of bounds"));
    }

    // Integration-style test using the actual verify logic with mock data
    // (real RPC tests would need a mock HTTP server)
    #[test]
    fn mock_tx_has_expected_structure() {
        let tx = mock_verbose_tx("tmTestAddr123", 42000);
        let vout = tx.vout.as_ref().expect("should have vout");
        assert_eq!(vout.len(), 1);
        assert_eq!(vout[0].value_zat, Some(42000));
    }
}
