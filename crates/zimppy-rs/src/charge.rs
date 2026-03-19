use std::future::Future;

use zimppy_core::{
    ConsumedTxids, TransparentVerifyRequest, VerifyError, ZebradRpc, verify_transparent,
};

/// Zcash charge verification method.
///
/// Verifies one-time Zcash payments by checking transparent transaction outputs
/// against expected address and amount.
///
/// Designed to implement the mpp-rs `ChargeMethod` trait when that dependency is added.
#[derive(Clone)]
pub struct ZcashChargeMethod {
    rpc: ZebradRpc,
    recipient: String,
    consumed: ConsumedTxids,
}

impl ZcashChargeMethod {
    pub fn new(rpc_endpoint: &str, recipient: &str) -> Self {
        Self {
            rpc: ZebradRpc::new(rpc_endpoint),
            recipient: recipient.to_string(),
            consumed: ConsumedTxids::new(),
        }
    }

    pub fn method(&self) -> &str {
        "zcash"
    }

    pub fn recipient(&self) -> &str {
        &self.recipient
    }

    /// Verify a transparent Zcash payment.
    ///
    /// This will be wired to `ChargeMethod::verify()` when mpp-rs is integrated.
    pub fn verify_payment(
        &self,
        txid: &str,
        output_index: u32,
        expected_amount_zat: u64,
    ) -> impl Future<Output = Result<VerifyOutcome, VerifyError>> + Send + '_ {
        let req = TransparentVerifyRequest {
            txid: txid.to_string(),
            output_index,
            expected_address: self.recipient.clone(),
            expected_amount_zat,
        };

        async move {
            let result = verify_transparent(&self.rpc, &req, &self.consumed).await?;

            Ok(VerifyOutcome {
                verified: result.verified,
                txid: result.txid,
                confirmations: result.confirmations,
            })
        }
    }
}

/// Outcome of a Zcash payment verification.
#[derive(Debug, Clone, serde::Serialize)]
pub struct VerifyOutcome {
    pub verified: bool,
    pub txid: String,
    pub confirmations: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_returns_zcash() {
        let method = ZcashChargeMethod::new("http://localhost:18232", "tmTestAddr");
        assert_eq!(method.method(), "zcash");
    }

    #[test]
    fn recipient_returns_configured_address() {
        let method = ZcashChargeMethod::new("http://localhost:18232", "tmMyAddr123");
        assert_eq!(method.recipient(), "tmMyAddr123");
    }
}
