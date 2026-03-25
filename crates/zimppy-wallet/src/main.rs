use std::path::PathBuf;

use zcash_protocol::consensus::NetworkType;
use zimppy_wallet::{WalletConfig, WalletError, ZimppyWallet};

#[tokio::main]
async fn main() -> Result<(), WalletError> {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("help");

    let data_dir = std::env::var("ZIMPPY_WALLET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_default()
                .join(".zimppy")
                .join("wallet")
        });

    let lwd = std::env::var("ZIMPPY_LWD_ENDPOINT")
        .unwrap_or_else(|_| "https://testnet.zec.rocks".to_string());

    let network = match std::env::var("ZIMPPY_NETWORK").as_deref() {
        Ok("mainnet") => NetworkType::Main,
        _ => NetworkType::Test,
    };

    let min_confirmations: Option<u32> = std::env::var("ZIMPPY_MIN_CONFIRMATIONS")
        .ok()
        .and_then(|s| s.parse().ok());

    match cmd {
        "init" => {
            let phrase = args.get(2).ok_or(WalletError::InvalidSeed(
                "usage: zimppy-wallet init \"seed phrase words...\"".to_string(),
            ))?;

            let birthday: Option<u32> = args.get(3).and_then(|s| s.parse().ok());

            let wallet = ZimppyWallet::create(WalletConfig {
                data_dir: data_dir.clone(),
                lwd_endpoint: lwd,
                network,
                seed_phrase: Some(phrase.clone()),
                birthday_height: birthday,
            })
            .await?;
            eprintln!("Wallet created at {}", data_dir.display());
            let addr = wallet.address().await?;
            println!("Address: {addr}");
        }

        "sync" => {
            let mut wallet = open_existing(&data_dir, &lwd, network).await?;
            if let Some(mc) = min_confirmations {
                wallet.set_min_confirmations(mc).await;
            }
            eprintln!("Syncing...");
            let status = wallet.sync().await?;
            eprintln!("Synced: {}", status.is_synced);

            let bal = wallet.balance().await?;
            println!(
                "Balance: {} zat (spendable: {}, pending: {})",
                bal.total_zat, bal.spendable_zat, bal.pending_zat
            );
            wallet.close_runtime().await?;
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
            let to = args.get(2).ok_or(WalletError::Send(
                "usage: zimppy-wallet send <address> <amount_zat> [memo]".to_string(),
            ))?;
            let amount: u64 = args
                .get(3)
                .and_then(|s| s.parse().ok())
                .ok_or(WalletError::Send("invalid amount".to_string()))?;
            let memo = args.get(4).map(|s| s.as_str());

            let mut wallet = open_existing(&data_dir, &lwd, network).await?;
            if let Some(mc) = min_confirmations {
                wallet.set_min_confirmations(mc).await;
            }
            let min_conf = wallet.min_confirmations().await;

            eprintln!("Syncing before send...");
            wallet.sync().await?;

            let bal = wallet.balance().await?;
            eprintln!(
                "Balance: spendable={} pending={} total={}",
                bal.spendable_zat, bal.pending_zat, bal.total_zat
            );

            let short = if to.len() > 25 { &to[..25] } else { to.as_str() };
            eprintln!("Sending {} zat to {}...", amount, short);

            // Send with retry: if change from a prior tx isn't mature enough,
            // the proposal fails with "insufficient balance" even though total is enough.
            // Retry until min_conf blocks pass and the change becomes spendable.
            let max_attempts = std::cmp::max(
                24usize,
                ((min_conf as u64) * BLOCK_TIME_SECS * 2 / POLL_INTERVAL_SECS) as usize,
            );
            let mut txid = None;
            for attempt in 0..=max_attempts {
                if attempt > 0 {
                    tokio::time::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;
                    wallet.sync().await?;
                    let bal = wallet.balance().await?;
                    eprint!(
                        "\r  [retry {attempt}/{max_attempts}] spendable={} pending={}    ",
                        bal.spendable_zat, bal.pending_zat
                    );
                    // Early exit: total balance genuinely too low
                    if bal.total_zat < amount + 10_000 {
                        eprintln!("\nInsufficient total balance.");
                        break;
                    }
                }
                match wallet.send(to, amount, memo).await {
                    Ok(id) => {
                        if attempt > 0 { eprintln!(); }
                        txid = Some(id);
                        break;
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        if msg.contains("Insufficient balance") && bal.total_zat >= amount + 10_000 {
                            if attempt == 0 {
                                eprintln!("Change not yet mature (min_conf={}), waiting...", min_conf);
                            }
                            continue;
                        }
                        return Err(e);
                    }
                }
            }
            let txid = txid.ok_or_else(|| {
                WalletError::Send("Timed out waiting for change to mature".to_string())
            })?;
            eprintln!("Broadcast: txid={txid}");

            // Post-send: wait for this tx's change to mature too
            eprintln!("Waiting for confirmation (min_conf={})...", min_conf);
            wait_for_send_maturity(&mut wallet, amount + 10_000, min_conf).await?;

            let bal = wallet.balance().await?;
            eprintln!(
                "Final: spendable={} pending={} total={}",
                bal.spendable_zat, bal.pending_zat, bal.total_zat
            );
            wallet.close_runtime().await?;
            println!("{txid}");
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
            eprintln!(
                "  ZIMPPY_LWD_ENDPOINT  Lightwalletd URL (default: https://testnet.zec.rocks)"
            );
            eprintln!("  ZIMPPY_NETWORK       mainnet or testnet (default: testnet)");
            eprintln!("  ZIMPPY_MIN_CONFIRMATIONS  Min confirmations for spendable (default: 3)");
        }
    }

    Ok(())
}

const POLL_INTERVAL_SECS: u64 = 15;
const BLOCK_TIME_SECS: u64 = 90; // ~75s target, 90s conservative

/// Wait until wallet has `needed` zats spendable (post-send maturity wait).
/// Upper-bounded by min_confirmations * block_time * 2.
/// Retries the actual send would work by checking spendable >= needed.
async fn wait_for_send_maturity(
    wallet: &mut ZimppyWallet,
    needed: u64,
    min_conf: u32,
) -> Result<(), WalletError> {
    let max_polls = std::cmp::max(
        20usize,
        ((min_conf as u64) * BLOCK_TIME_SECS * 2 / POLL_INTERVAL_SECS) as usize,
    );
    let mut stale_count = 0u32;
    let stale_limit = std::cmp::max(min_conf * 8, 24); // generous: ~2 full block cycles

    for attempt in 1..=max_polls {
        tokio::time::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;
        wallet.sync().await?;
        let bal = wallet.balance().await?;
        eprint!(
            "\r  [{attempt}/{max_polls}] spendable={} pending={}    ",
            bal.spendable_zat, bal.pending_zat
        );

        if bal.spendable_zat >= needed {
            eprintln!("\nSpendable!");
            return Ok(());
        }

        // Early exit: nothing in flight and balance genuinely too low
        if bal.pending_zat == 0 && bal.total_zat < needed {
            eprintln!("\nInsufficient total balance ({} < {}). Nothing to wait for.", bal.total_zat, needed);
            return Ok(());
        }

        // If pending=0 but spendable < needed, change is maturing (needs more confirmations)
        // Give it time proportional to min_conf before giving up
        if bal.pending_zat == 0 && bal.spendable_zat < needed {
            stale_count += 1;
            if stale_count > stale_limit {
                eprintln!("\nChange not maturing after {} polls. Proceeding.", stale_count);
                return Ok(());
            }
        } else {
            stale_count = 0;
        }
    }

    eprintln!("\nTimed out waiting for spendable balance.");
    Ok(())
}

async fn open_existing(
    data_dir: &PathBuf,
    lwd: &str,
    network: NetworkType,
) -> Result<ZimppyWallet, WalletError> {
    ZimppyWallet::open(WalletConfig {
        data_dir: data_dir.clone(),
        lwd_endpoint: lwd.to_string(),
        network,
        seed_phrase: None,
        birthday_height: None,
    })
    .await
}
