use napi_derive::napi;
use std::sync::Arc;

use zimppy_core::replay::ConsumedTxids;
use zimppy_core::rpc::ZebradRpc;
use zimppy_core::transparent::{self, TransparentVerifyRequest};

#[napi(object)]
pub struct NapiVerifyResult {
    pub verified: bool,
    pub txid: String,
    pub observed_address: String,
    pub observed_amount_zat: String,
    pub confirmations: u32,
}

#[napi]
pub struct ZimppyCore {
    rpc: Arc<ZebradRpc>,
    consumed: ConsumedTxids,
}

#[napi]
impl ZimppyCore {
    #[napi(constructor)]
    pub fn new(rpc_endpoint: String) -> Self {
        Self {
            rpc: Arc::new(ZebradRpc::new(&rpc_endpoint)),
            consumed: ConsumedTxids::new(),
        }
    }

    #[napi]
    pub async fn verify_transparent(
        &self,
        txid: String,
        output_index: u32,
        expected_address: String,
        expected_amount_zat: String,
    ) -> napi::Result<NapiVerifyResult> {
        let amount: u64 = expected_amount_zat
            .parse()
            .map_err(|_| napi::Error::from_reason("invalid amount: must be numeric string"))?;

        let req = TransparentVerifyRequest {
            txid,
            output_index,
            expected_address,
            expected_amount_zat: amount,
        };

        let result = transparent::verify_transparent(&self.rpc, &req, &self.consumed)
            .await
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;

        Ok(NapiVerifyResult {
            verified: result.verified,
            txid: result.txid,
            observed_address: result.observed_address,
            observed_amount_zat: result.observed_amount_zat.to_string(),
            confirmations: result.confirmations,
        })
    }

    #[napi]
    pub async fn health(&self) -> napi::Result<String> {
        Ok("{\"service\":\"zimppy-core\",\"status\":\"ok\"}".to_string())
    }
}
