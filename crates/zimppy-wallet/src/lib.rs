mod config;
mod error;

use std::num::NonZeroU32;

use bip0039::Mnemonic;
use zcash_address::ZcashAddress;
use zcash_protocol::consensus::BlockHeight;
use zcash_protocol::memo::MemoBytes;
use zcash_protocol::value::Zatoshis;
use zingolib::data::receivers::{Receiver, transaction_request_from_receivers};
use zingolib::lightclient::LightClient;
use zingolib::wallet::{LightWallet, WalletBase};

pub use config::WalletConfig;
pub use error::WalletError;

/// Wallet balance information.
#[derive(Debug, Clone)]
pub struct WalletBalance {
    /// Total spendable balance in zatoshis
    pub spendable_zat: u64,
    /// Balance pending confirmations
    pub pending_zat: u64,
    /// Total balance (spendable + pending)
    pub total_zat: u64,
}

/// Sync status after a sync operation.
#[derive(Debug)]
pub struct SyncStatus {
    pub is_synced: bool,
}

/// A native Zcash wallet backed by zingolib.
pub struct ZimppyWallet {
    client: LightClient,
}

impl ZimppyWallet {
    /// Open an existing wallet from disk, or create/restore from seed phrase.
    pub async fn open(wallet_config: WalletConfig) -> Result<Self, WalletError> {
        config::ensure_tls();
        let zingo_config = config::to_zingo_config(&wallet_config)?;

        // Try loading existing wallet first
        if wallet_config.data_dir.join("zingo-wallet.dat").exists() && wallet_config.seed_phrase.is_none() {
            let client = LightClient::create_from_wallet_path(zingo_config)
                .map_err(|e| WalletError::Client(format!("{e}")))?;
            return Ok(Self { client });
        }

        // Create from seed or fresh entropy
        let birthday = BlockHeight::from_u32(
            wallet_config.birthday_height.unwrap_or(3_906_900)
        );

        let wallet_base = match wallet_config.seed_phrase {
            Some(phrase) => {
                let mnemonic = Mnemonic::from_phrase(phrase)
                    .map_err(|e| WalletError::InvalidSeed(format!("{e}")))?;
                WalletBase::Mnemonic {
                    mnemonic,
                    no_of_accounts: NonZeroU32::new(1).expect("nonzero"),
                }
            }
            None => WalletBase::FreshEntropy {
                no_of_accounts: NonZeroU32::new(1).expect("nonzero"),
            },
        };

        let wallet = LightWallet::new(
            zingo_config.chain,
            wallet_base,
            birthday,
            zingo_config.wallet_settings.clone(),
        ).map_err(|e| WalletError::Client(format!("wallet creation failed: {e}")))?;

        let client = LightClient::create_from_wallet(wallet, zingo_config, true)
            .map_err(|e| WalletError::Client(format!("{e}")))?;

        Ok(Self { client })
    }

    /// Sync the wallet with the Zcash blockchain via lightwalletd.
    pub async fn sync(&mut self) -> Result<SyncStatus, WalletError> {
        self.client.sync_and_await().await
            .map_err(|e| WalletError::Sync(format!("{e}")))?;
        Ok(SyncStatus { is_synced: true })
    }

    /// Get the wallet's unified address.
    pub async fn address(&self) -> Result<String, WalletError> {
        let addrs = self.client.unified_addresses_json().await;
        addrs.members().next()
            .and_then(|a| a["encoded_address"].as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| WalletError::Address("no addresses found".to_string()))
    }

    /// Get the wallet's current balance.
    pub async fn balance(&self) -> Result<WalletBalance, WalletError> {
        let bal = self.client.account_balance(zip32::AccountId::ZERO).await
            .map_err(|e| WalletError::Client(format!("{e}")))?;

        let spendable = bal.confirmed_orchard_balance
            .map(u64::from)
            .unwrap_or(0);
        let pending = bal.unconfirmed_orchard_balance
            .map(u64::from)
            .unwrap_or(0);

        Ok(WalletBalance {
            spendable_zat: spendable,
            pending_zat: pending,
            total_zat: spendable + pending,
        })
    }

    /// Send ZEC to a recipient address with an optional memo.
    /// Returns the transaction ID as a hex string.
    pub async fn send(
        &mut self,
        to: &str,
        amount_zat: u64,
        memo: Option<&str>,
    ) -> Result<String, WalletError> {
        let recipient: ZcashAddress = to.parse()
            .map_err(|e| WalletError::Send(format!("invalid address: {e}")))?;

        let amount = Zatoshis::from_u64(amount_zat)
            .map_err(|_| WalletError::Send("invalid amount".to_string()))?;

        let memo_bytes = match memo {
            Some(m) => Some(MemoBytes::from_bytes(m.as_bytes())
                .map_err(|e| WalletError::Send(format!("memo error: {e}")))?),
            None => None,
        };

        let receivers = vec![Receiver {
            recipient_address: recipient,
            amount,
            memo: memo_bytes,
        }];

        let request = transaction_request_from_receivers(receivers)
            .map_err(|e| WalletError::Send(format!("request error: {e}")))?;

        let txids = self.client.quick_send(request, zip32::AccountId::ZERO, true).await
            .map_err(|e| WalletError::Send(format!("{e}")))?;

        Ok(txids.head.to_string())
    }

    /// Save wallet state to disk.
    pub async fn save(&self) -> Result<(), WalletError> {
        let wallet_path = self.client.config().get_wallet_path();
        let bytes = self.client.wallet.write().await.save()
            .map_err(|e| WalletError::Io(e))?;
        if let Some(data) = bytes {
            std::fs::write(&wallet_path, &data)
                .map_err(|e| WalletError::Io(e))?;
        }
        Ok(())
    }

    /// Get the wallet's seed phrase (if available).
    pub async fn seed_phrase(&self) -> Option<String> {
        self.client.wallet.read().await.mnemonic_phrase()
    }

    /// Get the network name ("testnet" or "mainnet").
    pub fn network(&self) -> &str {
        // Access via the config's chain type
        match self.client.config().chain {
            zingolib::config::ChainType::Mainnet => "mainnet",
            _ => "testnet",
        }
    }
}
