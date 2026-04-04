/// Wallet errors.
#[derive(Debug, thiserror::Error)]
pub enum WalletError {
    #[error("wallet error: {0}")]
    Client(String),
    #[error("wallet not initialized — run wallet login first")]
    NotInitialized,
    #[error("invalid seed phrase: {0}")]
    InvalidSeed(String),
    #[error("sync error: {0}")]
    Sync(String),
    #[error("send error: {0}")]
    Send(String),
    #[error("shield error: {0}")]
    Shield(String),
    #[error("address error: {0}")]
    Address(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("crypto error: {0}")]
    Crypto(String),
}
