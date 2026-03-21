use std::path::PathBuf;

use zimppy_wallet::{WalletConfig, WalletError, ZimppyWallet};
use zcash_protocol::consensus::NetworkType;

#[tokio::main]
async fn main() -> Result<(), WalletError> {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("help");

    let data_dir = std::env::var("ZIMPPY_WALLET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap_or_default().join(".zimppy").join("wallet"));

    let lwd = std::env::var("ZIMPPY_LWD_ENDPOINT")
        .unwrap_or_else(|_| "https://testnet.zec.rocks".to_string());

    let network = match std::env::var("ZIMPPY_NETWORK").as_deref() {
        Ok("mainnet") => NetworkType::Main,
        _ => NetworkType::Test,
    };

    match cmd {
        "init" => {
            let phrase = args.get(2)
                .ok_or(WalletError::InvalidSeed("usage: zimppy-wallet init \"seed phrase words...\"".to_string()))?;

            let birthday: Option<u32> = args.get(3).and_then(|s| s.parse().ok());

            let wallet = ZimppyWallet::open(WalletConfig {
                data_dir: data_dir.clone(),
                lwd_endpoint: lwd,
                network,
                seed_phrase: Some(phrase.clone()),
                birthday_height: birthday,
            }).await?;

            wallet.save().await?;
            eprintln!("Wallet created at {}", data_dir.display());
            let addr = wallet.address().await?;
            println!("Address: {addr}");
        }

        "sync" => {
            let mut wallet = open_existing(&data_dir, &lwd, network).await?;
            eprintln!("Syncing...");
            let status = wallet.sync().await?;
            wallet.save().await?;
            eprintln!("Synced: {}", status.is_synced);

            let bal = wallet.balance().await?;
            println!("Balance: {} zat (spendable: {}, pending: {})",
                bal.total_zat, bal.spendable_zat, bal.pending_zat);
        }

        "address" => {
            let wallet = open_existing(&data_dir, &lwd, network).await?;
            let addr = wallet.address().await?;
            println!("{addr}");
        }

        "balance" => {
            let wallet = open_existing(&data_dir, &lwd, network).await?;
            let bal = wallet.balance().await?;
            println!("Spendable: {} zat", bal.spendable_zat);
            println!("Pending:   {} zat", bal.pending_zat);
            println!("Total:     {} zat", bal.total_zat);
        }

        "send" => {
            let to = args.get(2)
                .ok_or(WalletError::Send("usage: zimppy-wallet send <address> <amount_zat> [memo]".to_string()))?;
            let amount: u64 = args.get(3)
                .and_then(|s| s.parse().ok())
                .ok_or(WalletError::Send("invalid amount".to_string()))?;
            let memo = args.get(4).map(|s| s.as_str());

            let mut wallet = open_existing(&data_dir, &lwd, network).await?;
            eprintln!("Sending {} zat to {}...", amount, to);
            let txid = wallet.send(to, amount, memo).await?;
            println!("Sent: {txid}");
        }

        _ => {
            eprintln!("zimppy-wallet — native Zcash wallet CLI");
            eprintln!();
            eprintln!("Commands:");
            eprintln!("  init <seed_phrase> [birthday]  Create/restore wallet from seed");
            eprintln!("  sync                           Sync with blockchain");
            eprintln!("  address                        Show unified address");
            eprintln!("  balance                        Show balance");
            eprintln!("  send <addr> <zat> [memo]       Send ZEC");
            eprintln!();
            eprintln!("Environment:");
            eprintln!("  ZIMPPY_WALLET_DIR    Data directory (default: ~/.zimppy/wallet)");
            eprintln!("  ZIMPPY_LWD_ENDPOINT  Lightwalletd URL (default: https://testnet.zec.rocks)");
            eprintln!("  ZIMPPY_NETWORK       mainnet or testnet (default: testnet)");
        }
    }

    Ok(())
}

async fn open_existing(data_dir: &PathBuf, lwd: &str, network: NetworkType) -> Result<ZimppyWallet, WalletError> {
    ZimppyWallet::open(WalletConfig {
        data_dir: data_dir.clone(),
        lwd_endpoint: lwd.to_string(),
        network,
        seed_phrase: None,
        birthday_height: None,
    }).await
}
