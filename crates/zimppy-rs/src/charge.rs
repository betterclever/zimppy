use std::future::Future;

use zimppy_core::replay::ConsumedTxids;
use zimppy_core::rpc::ZebradRpc;
use zimppy_core::shielded::{self, ShieldedVerifyRequest, ShieldedVerifyError};

/// Zcash charge verification method using Orchard shielded transactions.
///
/// Verifies payments by decrypting Orchard actions with the server's
/// Incoming Viewing Key (IVK), checking amount and memo binding.
#[derive(Clone)]
pub struct ZcashChargeMethod {
    rpc: ZebradRpc,
    recipient: String,
    orchard_ivk: String,
    consumed: ConsumedTxids,
}

impl ZcashChargeMethod {
    pub fn new(rpc_endpoint: &str, recipient: &str, orchard_ivk: &str) -> Self {
        Self {
            rpc: ZebradRpc::new(rpc_endpoint),
            recipient: recipient.to_string(),
            orchard_ivk: orchard_ivk.to_string(),
            consumed: ConsumedTxids::new(),
        }
    }

    pub fn method(&self) -> &str {
        "zcash"
    }

    pub fn recipient(&self) -> &str {
        &self.recipient
    }

    /// Verify a shielded Zcash payment.
    ///
    /// Decrypts Orchard actions with the server's IVK, checks:
    /// - Amount >= expected
    /// - Memo contains `zimppy:{challenge_id}`
    /// - Txid not replayed
    pub fn verify_payment(
        &self,
        txid: &str,
        challenge_id: &str,
        expected_amount_zat: u64,
    ) -> impl Future<Output = Result<VerifyOutcome, ShieldedVerifyError>> + Send + '_ {
        let req = ShieldedVerifyRequest {
            txid: txid.to_string(),
            ivk_bytes_hex: self.orchard_ivk.clone(),
            expected_challenge_id: challenge_id.to_string(),
            expected_amount_zat,
        };

        async move {
            let result = shielded::verify_shielded(&self.rpc, &req, &self.consumed).await?;

            Ok(VerifyOutcome {
                verified: result.verified,
                txid: result.txid,
                observed_amount_zat: result.observed_amount_zat,
                memo_matched: result.memo_matched,
                outputs_decrypted: result.outputs_decrypted,
            })
        }
    }
}

/// Outcome of a Zcash payment verification.
#[derive(Debug, Clone, serde::Serialize)]
pub struct VerifyOutcome {
    pub verified: bool,
    pub txid: String,
    pub observed_amount_zat: u64,
    pub memo_matched: bool,
    pub outputs_decrypted: u32,
}

// ── Item 4: mpp-rs ChargeMethod trait implementation ────────────────────

impl mpp::protocol::traits::ChargeMethod for ZcashChargeMethod {
    fn method(&self) -> &str {
        "zcash"
    }

    fn verify(
        &self,
        credential: &mpp::protocol::core::PaymentCredential,
        request: &mpp::protocol::intents::ChargeRequest,
    ) -> impl std::future::Future<Output = Result<mpp::protocol::core::Receipt, mpp::protocol::traits::VerificationError>> + Send {
        let credential = credential.clone();
        let amount: u64 = request.amount.parse().unwrap_or(0);
        let rpc = self.rpc.clone();
        let ivk = self.orchard_ivk.clone();
        let consumed = self.consumed.clone();

        async move {
            let txid = credential.charge_payload()
                .map(|p| p.data().to_string())
                .map_err(|e| mpp::protocol::traits::VerificationError::new(
                    format!("failed to parse credential payload: {e}")
                ))?;
            let challenge_id = credential.challenge.id.clone();
            let req = shielded::ShieldedVerifyRequest {
                txid: txid.clone(),
                ivk_bytes_hex: ivk,
                expected_challenge_id: challenge_id,
                expected_amount_zat: amount,
            };

            let result = shielded::verify_shielded(&rpc, &req, &consumed)
                .await
                .map_err(|e| mpp::protocol::traits::VerificationError::new(e.to_string()))?;

            if result.verified {
                Ok(mpp::protocol::core::Receipt::success("zcash", &result.txid))
            } else {
                Err(mpp::protocol::traits::VerificationError::new(
                    "payment not verified: amount or memo mismatch",
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_returns_zcash() {
        let method = ZcashChargeMethod::new("http://localhost:18232", "utest1...", "deadbeef");
        assert_eq!(method.method(), "zcash");
    }

    #[test]
    fn recipient_returns_configured_address() {
        let method = ZcashChargeMethod::new("http://localhost:18232", "utest1abc", "deadbeef");
        assert_eq!(method.recipient(), "utest1abc");
    }
}
