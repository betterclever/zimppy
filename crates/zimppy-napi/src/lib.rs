use napi_derive::napi;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;

use zimppy_core::replay::ConsumedTxids;
use zimppy_core::rpc::ZebradRpc;
use zimppy_core::shielded::{self, ShieldedVerifyRequest};
use zimppy_core::transparent::{self, TransparentVerifyRequest};
use zimppy_wallet::{WalletConfig, WalletError, ZimppyWallet};

#[napi(object)]
pub struct NapiVerifyResult {
    pub verified: bool,
    pub txid: String,
    pub observed_address: String,
    pub observed_amount_zat: String,
    pub confirmations: u32,
}

#[napi(object)]
pub struct NapiShieldedVerifyResult {
    pub verified: bool,
    pub txid: String,
    pub observed_amount_zat: String,
    pub memo_matched: bool,
    pub outputs_decrypted: u32,
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
    pub async fn verify_shielded(
        &self,
        txid: String,
        orchard_ivk: String,
        expected_challenge_id: String,
        expected_amount_zat: String,
    ) -> napi::Result<NapiShieldedVerifyResult> {
        let amount: u64 = expected_amount_zat
            .parse()
            .map_err(|_| napi::Error::from_reason("invalid amount: must be numeric string"))?;

        let req = ShieldedVerifyRequest {
            txid,
            ivk_bytes_hex: orchard_ivk,
            expected_challenge_id,
            expected_amount_zat: amount,
        };

        let result = shielded::verify_shielded(&self.rpc, &req, &self.consumed)
            .await
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;

        Ok(NapiShieldedVerifyResult {
            verified: result.verified,
            txid: result.txid,
            observed_amount_zat: result.observed_amount_zat.to_string(),
            memo_matched: result.memo_matched,
            outputs_decrypted: result.outputs_decrypted,
        })
    }

    #[napi]
    pub async fn health(&self) -> napi::Result<String> {
        Ok("{\"service\":\"zimppy-core\",\"status\":\"ok\"}".to_string())
    }
}

// ── Wallet NAPI bindings ────────────────────────────────────────────

#[napi(object)]
pub struct NapiWalletBalance {
    pub spendable_zat: String,
    pub pending_zat: String,
    pub total_zat: String,
}

#[napi]
pub struct ZimppyWalletNapi {
    wallet: Arc<TokioMutex<ZimppyWallet>>,
    network_name: String,
}

#[napi]
impl ZimppyWalletNapi {
    /// Open or create a wallet.
    /// Pass seed_phrase to create/restore, or omit to load existing.
    #[napi(factory)]
    pub async fn open(
        data_dir: String,
        lwd_endpoint: String,
        network: String,
        seed_phrase: Option<String>,
        birthday_height: Option<u32>,
    ) -> napi::Result<Self> {
        let network_type = match network.as_str() {
            "mainnet" => zcash_protocol::consensus::NetworkType::Main,
            _ => zcash_protocol::consensus::NetworkType::Test,
        };

        let wallet = ZimppyWallet::open(WalletConfig {
            data_dir: PathBuf::from(data_dir),
            lwd_endpoint,
            network: network_type,
            seed_phrase,
            birthday_height,
        })
        .await
        .map_err(|e: WalletError| napi::Error::from_reason(e.to_string()))?;

        let network_name = wallet.network().to_string();
        Ok(Self {
            wallet: Arc::new(TokioMutex::new(wallet)),
            network_name,
        })
    }

    /// Sync the wallet with the blockchain.
    #[napi]
    pub async fn sync(&self) -> napi::Result<bool> {
        let mut wallet = self.wallet.lock().await;
        let status = wallet
            .sync()
            .await
            .map_err(|e: WalletError| napi::Error::from_reason(e.to_string()))?;
        wallet
            .save()
            .await
            .map_err(|e: WalletError| napi::Error::from_reason(e.to_string()))?;
        Ok(status.is_synced)
    }

    /// Get the wallet's unified address.
    #[napi]
    pub async fn address(&self) -> napi::Result<String> {
        let wallet = self.wallet.lock().await;
        wallet
            .address()
            .await
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }

    /// Get the wallet balance.
    #[napi]
    pub async fn balance(&self) -> napi::Result<NapiWalletBalance> {
        let wallet = self.wallet.lock().await;
        let bal = wallet
            .balance()
            .await
            .map_err(|e: WalletError| napi::Error::from_reason(e.to_string()))?;
        Ok(NapiWalletBalance {
            spendable_zat: bal.spendable_zat.to_string(),
            pending_zat: bal.pending_zat.to_string(),
            total_zat: bal.total_zat.to_string(),
        })
    }

    /// Send ZEC to an address with optional memo. Returns txid.
    #[napi]
    pub async fn send(
        &self,
        to: String,
        amount_zat: String,
        memo: Option<String>,
    ) -> napi::Result<String> {
        let amount: u64 = amount_zat
            .parse()
            .map_err(|_| napi::Error::from_reason("invalid amount"))?;
        let mut wallet = self.wallet.lock().await;
        let txid = wallet
            .send(&to, amount, memo.as_deref())
            .await
            .map_err(|e: WalletError| napi::Error::from_reason(e.to_string()))?;
        wallet
            .save()
            .await
            .map_err(|e: WalletError| napi::Error::from_reason(e.to_string()))?;
        Ok(txid)
    }

    /// Get the wallet's seed phrase (if available).
    #[napi]
    pub async fn seed_phrase(&self) -> Option<String> {
        let wallet = self.wallet.lock().await;
        wallet.seed_phrase().await
    }

    /// Get the Orchard Incoming Viewing Key (IVK) as a hex string.
    #[napi]
    pub async fn orchard_ivk(&self) -> napi::Result<String> {
        let wallet = self.wallet.lock().await;
        wallet
            .orchard_ivk()
            .await
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }

    /// Get the network name.
    #[napi]
    pub fn network(&self) -> String {
        self.network_name.clone()
    }
}
