use napi_derive::napi;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
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
    closed: Arc<TokioMutex<bool>>,
}

fn trace_napi(event: &str, details: impl std::fmt::Display) {
    tracing::debug!(
        "[zimppy-napi:{event}] pid={} ts_ms={} {details}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_millis()
    );
}

#[napi]
impl ZimppyWalletNapi {
    /// Open an existing wallet.
    #[napi(factory)]
    pub async fn open(
        data_dir: String,
        lwd_endpoint: String,
        network: String,
    ) -> napi::Result<Self> {
        trace_napi(
            "open",
            format!("data_dir={} lwd_endpoint={} network={}", data_dir, lwd_endpoint, network),
        );
        Self::open_inner(data_dir, lwd_endpoint, network, None, None, false).await
    }

    /// Create a fresh wallet and persist it immediately.
    #[napi(factory)]
    pub async fn create(
        data_dir: String,
        lwd_endpoint: String,
        network: String,
        birthday_height: Option<u32>,
    ) -> napi::Result<Self> {
        Self::open_inner(data_dir, lwd_endpoint, network, None, birthday_height, true).await
    }

    /// Restore a wallet from a seed phrase and persist it immediately.
    #[napi(factory)]
    pub async fn restore(
        data_dir: String,
        lwd_endpoint: String,
        network: String,
        seed_phrase: String,
        birthday_height: u32,
    ) -> napi::Result<Self> {
        Self::open_inner(
            data_dir,
            lwd_endpoint,
            network,
            Some(seed_phrase),
            Some(birthday_height),
            true,
        )
        .await
    }

    async fn open_inner(
        data_dir: String,
        lwd_endpoint: String,
        network: String,
        seed_phrase: Option<String>,
        birthday_height: Option<u32>,
        create: bool,
    ) -> napi::Result<Self> {
        let network_type = match network.as_str() {
            "mainnet" => zcash_protocol::consensus::NetworkType::Main,
            _ => zcash_protocol::consensus::NetworkType::Test,
        };

        let wallet_config = WalletConfig {
            data_dir: PathBuf::from(data_dir),
            lwd_endpoint,
            network: network_type,
            seed_phrase,
            birthday_height,
            account_index: 0,
            num_accounts: 1,
        };

        let wallet = if create {
            ZimppyWallet::create(wallet_config).await
        } else {
            ZimppyWallet::open(wallet_config).await
        }
        .map_err(|e: WalletError| napi::Error::from_reason(e.to_string()))?;

        let network_name = wallet.network().to_string();
        let wallet = Arc::new(TokioMutex::new(wallet));
        {
            let mut guard = wallet.lock().await;
            trace_napi("open_inner:start_runtime", "starting runtime");
            guard
                .start_runtime()
                .await
                .map_err(|e: WalletError| napi::Error::from_reason(e.to_string()))?;
        }

        Ok(Self {
            wallet,
            network_name,
            closed: Arc::new(TokioMutex::new(false)),
        })
    }

    async fn ensure_open(&self) -> napi::Result<()> {
        let closed = self.closed.lock().await;
        if *closed {
            Err(napi::Error::from_reason("wallet runtime is closed"))
        } else {
            Ok(())
        }
    }

    /// Sync the wallet with the blockchain.
    #[napi]
    pub async fn sync(&self) -> napi::Result<bool> {
        self.ensure_open().await?;
        let mut wallet = self.wallet.lock().await;
        let status = wallet
            .sync()
            .await
            .map_err(|e: WalletError| napi::Error::from_reason(e.to_string()))?;
        Ok(status.is_synced)
    }

    #[napi]
    pub async fn ensure_ready(&self) -> napi::Result<bool> {
        self.ensure_open().await?;
        trace_napi("ensure_ready", "begin");
        let mut wallet = self.wallet.lock().await;
        wallet
            .ensure_ready()
            .await
            .map_err(|e: WalletError| napi::Error::from_reason(e.to_string()))?;
        trace_napi("ensure_ready", "end");
        Ok(true)
    }

    /// Get the wallet's default unified address.
    #[napi]
    pub async fn address(&self) -> napi::Result<String> {
        self.ensure_open().await?;
        let wallet = self.wallet.lock().await;
        wallet
            .address()
            .await
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }

    /// Get the wallet balance.
    #[napi]
    pub async fn balance(&self) -> napi::Result<NapiWalletBalance> {
        self.ensure_open().await?;
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
        self.ensure_open().await?;
        trace_napi(
            "send",
            format!("to={} amount_zat={} memo={:?}", to, amount_zat, memo),
        );
        let amount: u64 = amount_zat
            .parse()
            .map_err(|_| napi::Error::from_reason("invalid amount"))?;
        let mut wallet = self.wallet.lock().await;
        let txid = wallet
            .send(&to, amount, memo.as_deref())
            .await
            .map_err(|e: WalletError| napi::Error::from_reason(e.to_string()))?;
        trace_napi("send", format!("txid={txid}"));
        Ok(txid)
    }

    /// Get the wallet's seed phrase (if available).
    #[napi]
    pub async fn seed_phrase(&self) -> Option<String> {
        if self.ensure_open().await.is_err() {
            return None;
        }
        let wallet = self.wallet.lock().await;
        wallet.seed_phrase().await
    }

    /// Generate a unified address with both Sapling + Orchard receivers.
    #[napi]
    pub async fn full_address(&self) -> napi::Result<String> {
        self.ensure_open().await?;
        let mut wallet = self.wallet.lock().await;
        wallet
            .full_address()
            .await
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }

    /// Rescan the blockchain from birthday, rebuilding shard tree checkpoints.
    #[napi]
    pub async fn rescan(&self) -> napi::Result<()> {
        self.ensure_open().await?;
        let mut wallet = self.wallet.lock().await;
        wallet
            .rescan()
            .await
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }

    /// Get the Orchard Incoming Viewing Key (IVK) as a hex string.
    #[napi]
    pub async fn orchard_ivk(&self) -> napi::Result<String> {
        self.ensure_open().await?;
        let wallet = self.wallet.lock().await;
        wallet
            .orchard_ivk()
            .await
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }

    #[napi]
    pub async fn set_min_confirmations(&self, min_conf: u32) -> napi::Result<()> {
        self.ensure_open().await?;
        let wallet = self.wallet.lock().await;
        wallet.set_min_confirmations(min_conf).await;
        Ok(())
    }

    #[napi]
    pub async fn close(&self) -> napi::Result<()> {
        trace_napi("close", "begin");
        let mut closed = self.closed.lock().await;
        if *closed {
            trace_napi("close", "already_closed");
            return Ok(());
        }

        let mut wallet = self.wallet.lock().await;
        wallet
            .close_runtime()
            .await
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        *closed = true;
        trace_napi("close", "end");
        Ok(())
    }

    /// Get the network name.
    #[napi]
    pub fn network(&self) -> String {
        self.network_name.clone()
    }
}
