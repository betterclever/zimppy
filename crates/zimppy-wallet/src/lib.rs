mod config;
mod encryption;
mod error;

use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Cursor, Read, Write};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use bip0039::Mnemonic;
use zcash_address::ZcashAddress;
use zcash_protocol::consensus::BlockHeight;
use zcash_protocol::memo::MemoBytes;
use zcash_protocol::value::Zatoshis;
use zingolib::data::receivers::{transaction_request_from_receivers, Receiver};
use zingolib::grpc_connector;
use zingolib::lightclient::error::LightClientError;
use zingolib::lightclient::LightClient;
use zingolib::wallet::{LightWallet, WalletBase};
use shardtree::store::ShardStore as _;

pub use config::WalletConfig;
pub use error::WalletError;

/// Wallet balance information.
#[derive(Debug, Clone)]
pub struct WalletBalance {
    /// Spendable shielded (Orchard) balance in zatoshis
    pub spendable_zat: u64,
    /// Shielded balance pending confirmations
    pub pending_zat: u64,
    /// Total shielded balance (spendable + pending)
    pub total_zat: u64,
    /// Confirmed transparent balance in zatoshis
    pub transparent_zat: u64,
    /// Unconfirmed transparent balance in zatoshis
    pub transparent_pending_zat: u64,
}

/// Sync status after a sync operation.
#[derive(Debug)]
pub struct SyncStatus {
    pub is_synced: bool,
}

/// A native Zcash wallet backed by zingolib.
pub struct ZimppyWallet {
    client: LightClient,
    save_task_running: bool,
    account_index: u32,
    passphrase: Option<String>,
}

impl ZimppyWallet {
    fn account_id(&self) -> zip32::AccountId {
        zip32::AccountId::try_from(self.account_index).unwrap_or(zip32::AccountId::ZERO)
    }
}

fn trace_id() -> String {
    format!(
        "pid={} ts_ms={}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_millis()
    )
}

fn trace_wallet(event: &str, details: impl std::fmt::Display) {
    tracing::debug!("[zimppy-wallet:{event}] {} {details}", trace_id());
}

impl ZimppyWallet {
    /// Open an existing wallet from disk.
    ///
    /// If the wallet file does not exist, this returns [`WalletError::NotInitialized`]
    /// unless a seed phrase is provided, in which case the wallet is restored.
    pub async fn open(wallet_config: WalletConfig) -> Result<Self, WalletError> {
        trace_wallet(
            "open:begin",
            format!(
                "data_dir={} exists={} seed_present={} birthday={:?}",
                wallet_config.data_dir.display(),
                wallet_path(&wallet_config.data_dir).exists(),
                wallet_config.seed_phrase.is_some(),
                wallet_config.birthday_height
            ),
        );
        if wallet_path(&wallet_config.data_dir).exists() {
            return Self::open_existing(wallet_config);
        }

        if wallet_config.seed_phrase.is_some() {
            return Self::create(wallet_config).await;
        }

        Err(WalletError::NotInitialized)
    }

    /// Create a new wallet or restore one from seed, then persist it immediately.
    pub async fn create(wallet_config: WalletConfig) -> Result<Self, WalletError> {
        trace_wallet(
            "create:begin",
            format!(
                "data_dir={} wallet_exists={} seed_present={} birthday={:?}",
                wallet_config.data_dir.display(),
                wallet_path(&wallet_config.data_dir).exists(),
                wallet_config.seed_phrase.is_some(),
                wallet_config.birthday_height
            ),
        );
        config::ensure_tls();
        let zingo_config = config::to_zingo_config(&wallet_config)?;
        let wallet_path = wallet_path(&wallet_config.data_dir);

        if wallet_path.exists() {
            return Err(WalletError::Client(format!(
                "wallet already exists at {}",
                wallet_path.display()
            )));
        }

        let client = match wallet_config.seed_phrase {
            Some(phrase) => {
                let birthday_height = wallet_config.birthday_height.ok_or_else(|| {
                    WalletError::Client("restoring from seed requires birthday_height".to_string())
                })?;
                let mnemonic = Mnemonic::from_phrase(phrase)
                    .map_err(|e| WalletError::InvalidSeed(format!("{e}")))?;
                let wallet_base = WalletBase::Mnemonic {
                    mnemonic,
                    no_of_accounts: NonZeroU32::new(wallet_config.num_accounts.max(1))
                        .expect("nonzero"),
                };
                LightWallet::new(
                    zingo_config.chain,
                    wallet_base,
                    BlockHeight::from_u32(birthday_height),
                    zingo_config.wallet_settings.clone(),
                )
                .map_err(|e| WalletError::Client(format!("wallet creation failed: {e}")))
                .and_then(|wallet| {
                    LightClient::create_from_wallet(wallet, zingo_config, false)
                        .map_err(|e| WalletError::Client(format!("{e}")))
                })?
            }
            None => {
                let chain_height =
                    grpc_connector::get_latest_block(zingo_config.get_lightwalletd_uri())
                        .await
                        .map(|block_id| BlockHeight::from_u32(block_id.height as u32))
                        .map_err(|e| {
                            WalletError::Client(format!("failed to fetch chain height: {e}"))
                        })?;

                LightClient::new(zingo_config, chain_height, false)
                    .map_err(|e| WalletError::Client(format!("{e}")))?
            }
        };

        let wallet = Self {
            client,
            save_task_running: false,
            account_index: wallet_config.account_index,
            passphrase: wallet_config.passphrase.clone(),
        };
        wallet.save().await?;
        trace_wallet(
            "create:end",
            format!("wallet_path={}", wallet.client.config().get_wallet_path().display()),
        );
        Ok(wallet)
    }

    fn open_existing(wallet_config: WalletConfig) -> Result<Self, WalletError> {
        config::ensure_tls();
        let account_index = wallet_config.account_index;
        let passphrase = wallet_config.passphrase.clone();
        let zingo_config = config::to_zingo_config(&wallet_config)?;
        let path = zingo_config.get_wallet_path();
        trace_wallet("open_existing", format!("wallet_path={}", path.display()));

        let mut raw = Vec::new();
        fs::File::open(&path)?.read_to_end(&mut raw)?;

        let plaintext = if encryption::is_encrypted(&raw) {
            let pp = passphrase.as_deref().ok_or_else(|| {
                WalletError::Crypto(
                    "wallet is encrypted — provide a passphrase to open it".to_string(),
                )
            })?;
            encryption::decrypt(pp, &raw)?
        } else {
            raw
        };

        let wallet = LightWallet::read(Cursor::new(plaintext), zingo_config.chain)
            .map_err(|e| WalletError::Client(format!("wallet read failed: {e}")))?;

        // overwrite=true skips the file-exists check; no write occurs here
        let client = LightClient::create_from_wallet(wallet, zingo_config, true)
            .map_err(|e| WalletError::Client(format!("{e}")))?;

        Ok(Self {
            client,
            save_task_running: false,
            account_index,
            passphrase,
        })
    }

    /// Sync the wallet with the Zcash blockchain via lightwalletd.
    pub async fn sync(&mut self) -> Result<SyncStatus, WalletError> {
        trace_wallet(
            "sync:begin",
            format!("wallet_path={}", self.client.config().get_wallet_path().display()),
        );
        self.ensure_ready().await?;
        trace_wallet(
            "sync:end",
            format!("wallet_path={}", self.client.config().get_wallet_path().display()),
        );
        Ok(SyncStatus { is_synced: true })
    }

    /// Get the wallet's default unified address (stable, index 0).
    pub async fn address(&self) -> Result<String, WalletError> {
        let addrs = self.client.unified_addresses_json().await;
        addrs
            .members()
            .next()
            .and_then(|a| a["encoded_address"].as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| WalletError::Address("no addresses found".to_string()))
    }

    /// Generate a unified address with both Sapling and Orchard receivers.
    /// Returns an existing full shielded UA if one was already created.
    pub async fn full_address(&mut self) -> Result<String, WalletError> {
        use zingolib::wallet::keys::unified::ReceiverSelection;

        if let Some(encoded) = self
            .client
            .unified_addresses_json()
            .await
            .members()
            .find(|address| {
                address["has_orchard"].as_bool() == Some(true)
                    && address["has_sapling"].as_bool() == Some(true)
            })
            .and_then(|address| address["encoded_address"].as_str())
        {
            return Ok(encoded.to_string());
        }

        let (_id, ua) = self
            .client
            .generate_unified_address(ReceiverSelection::all_shielded(), self.account_id())
            .await
            .map_err(|e| WalletError::Address(format!("failed to generate address: {e:?}")))?;

        self.save().await?;

        Ok(ua.encode(&self.client.config().chain))
    }

    /// Get the wallet's current balance.
    pub async fn balance(&self) -> Result<WalletBalance, WalletError> {
        let bal = self
            .client
            .account_balance(self.account_id())
            .await
            .map_err(|e| WalletError::Client(format!("{e}")))?;

        let spendable = bal.confirmed_orchard_balance.map(u64::from).unwrap_or(0);
        let pending = bal.unconfirmed_orchard_balance.map(u64::from).unwrap_or(0);
        let transparent = bal.confirmed_transparent_balance.map(u64::from).unwrap_or(0);
        let transparent_pending = bal.unconfirmed_transparent_balance.map(u64::from).unwrap_or(0);

        Ok(WalletBalance {
            spendable_zat: spendable,
            pending_zat: pending,
            total_zat: spendable + pending,
            transparent_zat: transparent,
            transparent_pending_zat: transparent_pending,
        })
    }

    /// Send ZEC to a recipient address with an optional memo.
    /// Returns the transaction ID as a hex string.
    ///
    /// Matches zingo-cli's send pattern exactly:
    /// - Does NOT await sync before send (quick_send pauses sync internally)
    /// - Does NOT manually save (background save_task handles persistence)
    /// - Just calls quick_send() directly, like zingo-cli's QuickSendCommand
    pub async fn send(
        &mut self,
        to: &str,
        amount_zat: u64,
        memo: Option<&str>,
    ) -> Result<String, WalletError> {
        trace_wallet(
            "send:begin",
            format!(
                "wallet_path={} to={} amount={} memo={:?}",
                self.client.config().get_wallet_path().display(),
                to,
                amount_zat,
                memo
            ),
        );
        let recipient: ZcashAddress = to
            .parse()
            .map_err(|e| WalletError::Send(format!("invalid address: {e}")))?;

        let amount = Zatoshis::from_u64(amount_zat)
            .map_err(|_| WalletError::Send("invalid amount".to_string()))?;

        let memo_bytes = match memo {
            Some(m) => Some(
                MemoBytes::from_bytes(m.as_bytes())
                    .map_err(|e| WalletError::Send(format!("memo error: {e}")))?,
            ),
            None => None,
        };

        let receivers = vec![Receiver {
            recipient_address: recipient,
            amount,
            memo: memo_bytes,
        }];

        let request = transaction_request_from_receivers(receivers)
            .map_err(|e| WalletError::Send(format!("request error: {e}")))?;

        // Debug: log checkpoint state before send
        {
            let wallet = self.client.wallet.read().await;
            let sapling_store = wallet.shard_trees.sapling.store();
            let orchard_store = wallet.shard_trees.orchard.store();
            let s_count = sapling_store.checkpoint_count().unwrap_or(0);
            let o_count = orchard_store.checkpoint_count().unwrap_or(0);
            let s_max = sapling_store.max_checkpoint_id().ok().flatten();
            let o_max = orchard_store.max_checkpoint_id().ok().flatten();
            let s_min = sapling_store.min_checkpoint_id().ok().flatten();
            let o_min = orchard_store.min_checkpoint_id().ok().flatten();
            let min_conf = wallet.wallet_settings.min_confirmations;
            let chain_h = wallet.sync_state.last_known_chain_height();
            trace_wallet(
                "send:checkpoints",
                format!(
                    "sapling=[{:?}..{:?}](count={}) orchard=[{:?}..{:?}](count={}) min_conf={} chain_height={:?}",
                    s_min, s_max, s_count, o_min, o_max, o_count, min_conf, chain_h
                ),
            );
        }

        let txids = self
            .client
            .quick_send(request, self.account_id(), true)
            .await
            .map_err(|e| WalletError::Send(format!("{e}")))?;

        trace_wallet(
            "send:end",
            format!(
                "wallet_path={} txid={}",
                self.client.config().get_wallet_path().display(),
                txids.head
            ),
        );
        Ok(txids.head.to_string())
    }

    /// Rescan the blockchain from birthday, rebuilding shard tree checkpoints.
    pub async fn rescan(&mut self) -> Result<(), WalletError> {
        self.ensure_save_task().await;
        self.client
            .rescan_and_await()
            .await
            .map_err(|e| WalletError::Sync(format!("rescan failed: {e}")))?;
        if self.passphrase.is_some() {
            self.client.wallet.write().await.save_required = true;
            self.save().await?;
        }
        Ok(())
    }

    /// Save wallet state to disk.
    /// If a passphrase is set, the wallet bytes are encrypted before writing.
    pub async fn save(&self) -> Result<(), WalletError> {
        let wallet_path = self.client.config().get_wallet_path();
        trace_wallet("save:begin", format!("wallet_path={}", wallet_path.display()));
        let bytes = self
            .client
            .wallet
            .write()
            .await
            .save()
            .map_err(|e| WalletError::Io(e))?;
        if let Some(plaintext) = bytes {
            let data = match self.passphrase.as_deref() {
                Some(pp) => {
                    trace_wallet(
                        "save:encrypt",
                        format!("wallet_path={} plaintext_bytes={}", wallet_path.display(), plaintext.len()),
                    );
                    encryption::encrypt(pp, &plaintext)?
                }
                None => plaintext,
            };
            trace_wallet(
                "save:write",
                format!("wallet_path={} bytes={}", wallet_path.display(), data.len()),
            );
            write_wallet_bytes(&wallet_path, &data)?;
        } else {
            trace_wallet("save:skip", format!("wallet_path={}", wallet_path.display()));
        }
        trace_wallet("save:end", format!("wallet_path={}", wallet_path.display()));
        Ok(())
    }

    /// Get the wallet's seed phrase (if available).
    pub async fn seed_phrase(&self) -> Option<String> {
        self.client.wallet.read().await.mnemonic_phrase()
    }

    /// Get the Orchard Incoming Viewing Key (IVK) as a hex string.
    /// Derived from the wallet's spending or viewing key.
    pub async fn orchard_ivk(&self) -> Result<String, WalletError> {
        use zcash_keys::keys::UnifiedFullViewingKey;
        use zip32::Scope;

        let wallet = self.client.wallet.read().await;
        let account_id = self.account_id();
        let key_store = wallet
            .unified_key_store
            .get(&account_id)
            .ok_or_else(|| WalletError::Address("no key store for account 0".to_string()))?;

        let ufvk = UnifiedFullViewingKey::try_from(key_store)
            .map_err(|e| WalletError::Address(format!("failed to derive UFVK: {e:?}")))?;

        let orchard_fvk = ufvk
            .orchard()
            .ok_or_else(|| WalletError::Address("no Orchard key in wallet".to_string()))?;

        let ivk = orchard_fvk.to_ivk(Scope::External);
        Ok(hex::encode(ivk.to_bytes()))
    }

    /// Return the active account index.
    pub fn account_index(&self) -> u32 {
        self.account_index
    }

    /// List (account_index, unified_address) for all accounts in this wallet.
    pub async fn accounts_list(&self) -> Result<Vec<(u32, String)>, WalletError> {
        let addrs = self.client.unified_addresses_json().await;
        let result: Vec<(u32, String)> = addrs
            .members()
            .enumerate()
            .filter_map(|(i, entry)| {
                entry["encoded_address"]
                    .as_str()
                    .map(|a| (i as u32, a.to_string()))
            })
            .collect();
        if result.is_empty() {
            return Err(WalletError::Address("no accounts found".to_string()));
        }
        Ok(result)
    }

    /// Set the minimum confirmations required before notes are spendable.
    /// This is persisted to disk with the wallet.
    pub async fn set_min_confirmations(&self, min_conf: u32) {
        let min_conf = std::num::NonZeroU32::new(min_conf).unwrap_or(std::num::NonZeroU32::new(1).expect("1 is nonzero"));
        self.client.wallet.write().await.wallet_settings.min_confirmations = min_conf;
    }

    /// Get the current min_confirmations setting.
    pub async fn min_confirmations(&self) -> u32 {
        self.client.wallet.read().await.wallet_settings.min_confirmations.get()
    }

    /// Get the network name ("testnet" or "mainnet").
    pub fn network(&self) -> &str {
        // Access via the config's chain type
        match self.client.config().chain {
            zingolib::config::ChainType::Mainnet => "mainnet",
            _ => "testnet",
        }
    }

    pub async fn start_runtime(&mut self) -> Result<(), WalletError> {
        trace_wallet(
            "runtime:start",
            format!("wallet_path={}", self.client.config().get_wallet_path().display()),
        );
        self.ensure_save_task().await;
        self.client
            .sync()
            .await
            .map_err(|e| WalletError::Sync(format!("{e}")))?;
        trace_wallet(
            "runtime:start:done",
            format!("wallet_path={}", self.client.config().get_wallet_path().display()),
        );
        Ok(())
    }

    pub async fn close_runtime(&mut self) -> Result<(), WalletError> {
        trace_wallet(
            "runtime:close",
            format!(
                "wallet_path={} save_task_running={}",
                self.client.config().get_wallet_path().display(),
                self.save_task_running
            ),
        );
        if self.save_task_running {
            // Wait for any pending save, then stop save_task
            self.client.wait_for_save().await;
            self.client
                .shutdown_save_task()
                .await
                .map_err(WalletError::Io)?;
            self.save_task_running = false;
        }
        // Always force a final save to persist latest checkpoint state
        self.client.wallet.write().await.save_required = true;
        self.save().await?;
        trace_wallet(
            "runtime:close:done",
            format!("wallet_path={}", self.client.config().get_wallet_path().display()),
        );
        Ok(())
    }

    pub async fn ensure_ready(&mut self) -> Result<(), WalletError> {
        trace_wallet(
            "ensure_ready:begin",
            format!("wallet_path={}", self.client.config().get_wallet_path().display()),
        );
        self.ensure_save_task().await;

        let sync_result = match self.client.await_sync().await {
            Ok(result) => Ok(result),
            Err(LightClientError::SyncNotRunning) => self.client.sync_and_await().await,
            Err(e) => Err(e),
        };

        sync_result.map_err(|e| WalletError::Sync(format!("{e}")))?;
        trace_wallet(
            "ensure_ready:end",
            format!("wallet_path={}", self.client.config().get_wallet_path().display()),
        );
        // For encrypted wallets, save_task is disabled — persist checkpoints explicitly.
        // For plaintext wallets, the background save_task handles persistence.
        if self.passphrase.is_some() {
            // Force save_required so save() actually writes.
            self.client.wallet.write().await.save_required = true;
            self.save().await?;
        }
        Ok(())
    }

    async fn ensure_save_task(&mut self) {
        // For encrypted wallets, never start zingolib's background save_task — it writes
        // raw plaintext bytes directly to disk, bypassing our encryption layer.
        // All persistence goes through ZimppyWallet::save() instead.
        if self.passphrase.is_some() {
            return;
        }
        if !self.save_task_running {
            trace_wallet(
                "save_task:start",
                format!("wallet_path={}", self.client.config().get_wallet_path().display()),
            );
            self.client.save_task().await;
            self.save_task_running = true;
        }
    }
}

fn wallet_path(data_dir: &Path) -> PathBuf {
    data_dir.join("zingo-wallet.dat")
}

fn write_wallet_bytes(wallet_path: &Path, bytes: &[u8]) -> Result<(), WalletError> {
    if let Some(parent) = wallet_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let temp_wallet_path = wallet_path.with_extension(
        wallet_path
            .extension()
            .map(|ext| format!("{}.tmp", ext.to_string_lossy()))
            .unwrap_or_else(|| "tmp".to_string()),
    );

    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temp_wallet_path)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(bytes)?;
    let file = writer
        .into_inner()
        .map_err(|e| WalletError::Io(e.into_error()))?;
    file.sync_all()?;
    fs::rename(&temp_wallet_path, wallet_path)?;

    #[cfg(unix)]
    if let Some(parent) = wallet_path.parent() {
        let wallet_dir = fs::File::open(parent)?;
        let _ignored = wallet_dir.sync_all();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{wallet_path, WalletBalance, WalletConfig, WalletError, ZimppyWallet};
    use bip0039::{English, Mnemonic};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};
    use zcash_protocol::consensus::NetworkType;

    fn test_wallet_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos();
        std::env::temp_dir().join(format!("zimppy-wallet-{name}-{unique}"))
    }

    fn test_config(
        data_dir: PathBuf,
        seed_phrase: Option<String>,
        birthday_height: Option<u32>,
    ) -> WalletConfig {
        WalletConfig {
            data_dir,
            lwd_endpoint: "https://testnet.zec.rocks".to_string(),
            network: NetworkType::Test,
            seed_phrase,
            birthday_height,
            account_index: 0,
            num_accounts: 1,
            passphrase: None,
        }
    }

    fn test_config_encrypted(
        data_dir: PathBuf,
        seed_phrase: Option<String>,
        birthday_height: Option<u32>,
        passphrase: &str,
    ) -> WalletConfig {
        WalletConfig {
            data_dir,
            lwd_endpoint: "https://testnet.zec.rocks".to_string(),
            network: NetworkType::Test,
            seed_phrase,
            birthday_height,
            account_index: 0,
            num_accounts: 1,
            passphrase: Some(passphrase.to_string()),
        }
    }

    #[test]
    fn encryption_round_trip() {
        use crate::encryption;
        let plaintext = b"hello zcash wallet bytes";
        let encrypted = encryption::encrypt("hunter2", plaintext).expect("encrypt");
        assert!(encryption::is_encrypted(&encrypted));
        let decrypted = encryption::decrypt("hunter2", &encrypted).expect("decrypt");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encryption_wrong_passphrase_fails() {
        use crate::encryption;
        let encrypted = encryption::encrypt("correct", b"secret").expect("encrypt");
        assert!(encryption::decrypt("wrong", &encrypted).is_err());
    }

    #[tokio::test]
    async fn encrypted_wallet_persists_and_reopens() {
        let data_dir = test_wallet_dir("enc-persist");
        let seed = Mnemonic::<English>::generate(bip0039::Count::Words24).to_string();

        let wallet = ZimppyWallet::create(test_config_encrypted(
            data_dir.clone(),
            Some(seed.clone()),
            Some(3_000_000),
            "mypassphrase",
        ))
        .await
        .expect("wallet should be created");
        let original_address = wallet.address().await.expect("address");

        // Verify the file on disk is actually encrypted
        let raw = std::fs::read(wallet_path(&data_dir)).expect("wallet file");
        assert!(crate::encryption::is_encrypted(&raw), "file should be encrypted");

        // Opening without passphrase must fail
        let no_pass_err = ZimppyWallet::open(test_config(data_dir.clone(), None, None)).await;
        assert!(matches!(no_pass_err, Err(WalletError::Crypto(_))));

        // Opening with correct passphrase must succeed and return same address
        let reopened = ZimppyWallet::open(test_config_encrypted(
            data_dir.clone(),
            None,
            None,
            "mypassphrase",
        ))
        .await
        .expect("wallet should reopen");
        let reopened_address = reopened.address().await.expect("address");
        assert_eq!(original_address, reopened_address);

        let _ = fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn open_without_wallet_file_returns_not_initialized() {
        let data_dir = test_wallet_dir("missing");
        let result = ZimppyWallet::open(test_config(data_dir, None, None)).await;
        assert!(matches!(result, Err(WalletError::NotInitialized)));
    }

    #[tokio::test]
    async fn create_persists_wallet_file_and_can_reopen() {
        let data_dir = test_wallet_dir("persist");
        let seed = Mnemonic::<English>::generate(bip0039::Count::Words24).to_string();

        let wallet = ZimppyWallet::create(test_config(
            data_dir.clone(),
            Some(seed.clone()),
            Some(3_000_000),
        ))
        .await
        .expect("wallet should be created");
        let original_address = wallet.address().await.expect("address");

        assert!(wallet_path(&data_dir).exists());

        let reopened = ZimppyWallet::open(test_config(data_dir.clone(), None, None))
            .await
            .expect("wallet should reopen");
        let reopened_address = reopened.address().await.expect("address");
        assert_eq!(original_address, reopened_address);

        let _ = fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn full_address_is_idempotent() {
        let data_dir = test_wallet_dir("full-address");
        let seed = Mnemonic::<English>::generate(bip0039::Count::Words24).to_string();

        let mut wallet =
            ZimppyWallet::create(test_config(data_dir.clone(), Some(seed), Some(3_000_000)))
                .await
                .expect("wallet should be created");

        let first = wallet.full_address().await.expect("first full address");
        let second = wallet.full_address().await.expect("second full address");
        assert_eq!(first, second);

        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn wallet_balance_has_transparent_fields() {
        let bal = WalletBalance {
            spendable_zat: 100,
            pending_zat: 0,
            total_zat: 100,
            transparent_zat: 50,
            transparent_pending_zat: 0,
        };
        assert_eq!(bal.transparent_zat, 50);
        assert_eq!(bal.transparent_pending_zat, 0);
    }
}
