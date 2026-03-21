use std::path::PathBuf;
use std::sync::Once;

use zingolib::config::{ChainType, ZingoConfig};
use zcash_protocol::consensus::NetworkType;

use crate::error::WalletError;

/// Configuration for opening or creating a wallet.
#[derive(Clone)]
pub struct WalletConfig {
    /// Directory for wallet data (SQLite db, logs)
    pub data_dir: PathBuf,
    /// Lightwalletd gRPC endpoint
    pub lwd_endpoint: String,
    /// Network (testnet or mainnet)
    pub network: NetworkType,
    /// BIP39 seed phrase (only needed for creating/restoring)
    pub seed_phrase: Option<String>,
    /// Birthday height (block to start scanning from)
    pub birthday_height: Option<u32>,
}

static TLS_INIT: Once = Once::new();

/// Ensure the rustls crypto provider is installed (call once at startup).
pub(crate) fn ensure_tls() {
    TLS_INIT.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Convert our config to a ZingoConfig.
pub(crate) fn to_zingo_config(config: &WalletConfig) -> Result<ZingoConfig, WalletError> {
    let chain = match config.network {
        NetworkType::Main => ChainType::Mainnet,
        NetworkType::Test => ChainType::Testnet,
        _ => ChainType::Testnet,
    };

    let uri = config.lwd_endpoint.parse()
        .map_err(|e| WalletError::Client(format!("invalid lightwalletd URI: {e}")))?;

    Ok(ZingoConfig::build(chain)
        .set_lightwalletd_uri(uri)
        .set_wallet_dir(config.data_dir.clone())
        .create())
}
