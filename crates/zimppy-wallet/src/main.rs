use std::path::PathBuf;
use std::time::Duration;

use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use zcash_protocol::consensus::NetworkType;
use zimppy_wallet::{WalletConfig, WalletError, ZimppyWallet};

// ── UI helpers ───────────────────────────────────────────────────

fn spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .expect("template")
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

fn format_zat(zat: u64) -> String {
    let zec = zat as f64 / 100_000_000.0;
    if zec >= 0.01 {
        format!("{} zat ({:.4} ZEC)", zat, zec)
    } else {
        format!("{} zat", zat)
    }
}

fn print_balance(bal: &zimppy_wallet::WalletBalance) {
    eprintln!("  Spendable: {}", style(format_zat(bal.spendable_zat)).green());
    if bal.pending_zat > 0 {
        eprintln!("  Pending:   {}", style(format_zat(bal.pending_zat)).yellow());
    }
    eprintln!("  Total:     {}", format_zat(bal.total_zat));
}

// ── Arg helpers ──────────────────────────────────────────────────

/// Return the value of `--flag <value>` from args, if present.
fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// Return positional arguments (those not starting with `--` and not values of flags).
fn positional_args(args: &[String]) -> Vec<&str> {
    let mut result = Vec::new();
    let mut skip_next = false;
    for arg in args.iter().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg.starts_with("--") {
            skip_next = true; // next arg is the flag value
            continue;
        }
        result.push(arg.as_str());
    }
    result
}

// ── Main ─────────────────────────────────────────────────────────

const POLL_INTERVAL: Duration = Duration::from_secs(15);
const BLOCK_TIME_SECS: u64 = 90;

#[tokio::main]
async fn main() -> Result<(), WalletError> {
    let args: Vec<String> = std::env::args().collect();
    let debug = args.contains(&"--debug".to_string());
    let pos = positional_args(&args);
    let cmd = pos.first().copied().unwrap_or("help");

    let data_dir = std::env::var("ZIMPPY_WALLET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_default()
                .join(".zimppy")
                .join("wallet")
        });

    // Tracing: file by default, stderr with --debug
    let log_dir = data_dir.join("logs");
    std::fs::create_dir_all(&log_dir).ok();
    if debug {
        tracing_subscriber::fmt::init();
    } else {
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.join("wallet.log"))
            .expect("failed to open log file");
        tracing_subscriber::fmt()
            .with_writer(std::sync::Mutex::new(log_file))
            .with_ansi(false)
            .init();
    }

    // --rpc overrides ZIMPPY_LWD_ENDPOINT
    let lwd = flag_value(&args, "--rpc")
        .or_else(|| std::env::var("ZIMPPY_LWD_ENDPOINT").ok())
        .unwrap_or_else(|| "https://testnet.zec.rocks".to_string());

    let network = match std::env::var("ZIMPPY_NETWORK").as_deref() {
        Ok("mainnet") => NetworkType::Main,
        _ => NetworkType::Test,
    };

    let min_confirmations: Option<u32> = std::env::var("ZIMPPY_MIN_CONFIRMATIONS")
        .ok()
        .and_then(|s| s.parse().ok());

    // --account <n> selects the account index (default 0)
    let account_index: u32 = flag_value(&args, "--account")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    match cmd {
        "init" => {
            let phrase = pos.get(1).ok_or(WalletError::InvalidSeed(
                "usage: zimppy-wallet init \"seed phrase words...\" <birthday> [--accounts N] [--rpc URL]".to_string(),
            ))?;
            let birthday: Option<u32> = pos.get(2).and_then(|s| s.parse().ok());
            let num_accounts: u32 = flag_value(&args, "--accounts")
                .and_then(|s| s.parse().ok())
                .unwrap_or(1);

            let sp = spinner("Creating wallet...");
            let wallet = ZimppyWallet::create(WalletConfig {
                data_dir: data_dir.clone(),
                lwd_endpoint: lwd,
                network,
                seed_phrase: Some(phrase.to_string()),
                birthday_height: birthday,
                account_index: 0,
                num_accounts,
            })
            .await?;
            sp.finish_and_clear();

            let addr = wallet.address().await?;
            eprintln!("{} Wallet created at {}", style("✓").green().bold(), data_dir.display());
            if num_accounts > 1 {
                eprintln!("  Accounts: {}", num_accounts);
            }
            eprintln!("  Address: {}", style(&addr).cyan());
            println!("{addr}");
        }

        "sync" => {
            let mut wallet = open_existing(&data_dir, &lwd, network, account_index).await?;
            if let Some(mc) = min_confirmations {
                wallet.set_min_confirmations(mc).await;
            }

            let sp = spinner("Syncing with blockchain...");
            wallet.sync().await?;
            sp.finish_and_clear();

            let bal = wallet.balance().await?;
            eprintln!("{} Synced", style("✓").green().bold());
            print_balance(&bal);
            wallet.close_runtime().await?;
        }

        "address" => {
            let wallet = open_existing(&data_dir, &lwd, network, account_index).await?;
            let addr = wallet.address().await?;
            println!("{addr}");
        }

        "balance" => {
            let mut wallet = open_existing(&data_dir, &lwd, network, account_index).await?;
            let sp = spinner("Syncing...");
            wallet.sync().await?;
            sp.finish_and_clear();
            let bal = wallet.balance().await?;
            print_balance(&bal);
            wallet.close_runtime().await?;
        }

        "send" => {
            let to = pos.get(1).ok_or(WalletError::Send(
                "usage: zimppy-wallet send <address> <amount_zat> [memo]".to_string(),
            ))?;
            let amount: u64 = pos
                .get(2)
                .and_then(|s| s.parse().ok())
                .ok_or(WalletError::Send("invalid amount".to_string()))?;
            let memo = pos.get(3).copied();

            let mut wallet = open_existing(&data_dir, &lwd, network, account_index).await?;
            if let Some(mc) = min_confirmations {
                wallet.set_min_confirmations(mc).await;
            }
            let min_conf = wallet.min_confirmations().await;

            let sp = spinner("Syncing...");
            wallet.sync().await?;
            sp.finish_and_clear();

            let bal = wallet.balance().await?;
            eprintln!("  Spendable: {}", style(format_zat(bal.spendable_zat)).cyan());

            let short = if to.len() > 40 {
                format!("{}...{}", &to[..20], &to[to.len()-12..])
            } else {
                to.to_string()
            };

            // Send with retry for change maturity
            let max_attempts = std::cmp::max(
                24usize,
                ((min_conf as u64) * BLOCK_TIME_SECS * 2 / 15) as usize,
            );
            let mut txid = None;
            let started = std::time::Instant::now();
            let sp = spinner(&format!("Sending {} to {}", format_zat(amount), short));

            for attempt in 0..=max_attempts {
                if attempt > 0 {
                    for _ in 0..POLL_INTERVAL.as_secs() {
                        let elapsed = started.elapsed().as_secs();
                        sp.set_message(format!(
                            "Waiting for prior change to mature... {}s elapsed", elapsed
                        ));
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                    wallet.sync().await?;
                    let b = wallet.balance().await?;
                    if b.total_zat < amount + 10_000 {
                        sp.finish_and_clear();
                        eprintln!("{} Insufficient balance", style("✗").red().bold());
                        break;
                    }
                }
                match wallet.send(to, amount, memo).await {
                    Ok(id) => {
                        txid = Some(id);
                        break;
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        if msg.contains("Insufficient balance") && bal.total_zat >= amount + 10_000 {
                            continue;
                        }
                        sp.finish_and_clear();
                        return Err(e);
                    }
                }
            }
            sp.finish_and_clear();

            let txid = txid.ok_or_else(|| {
                WalletError::Send("Timed out waiting for change to mature".to_string())
            })?;
            eprintln!("{} Broadcast: {}", style("✓").green().bold(), style(&txid).dim());

            let sp = spinner("Waiting for confirmation...");
            wait_for_maturity(&mut wallet, amount + 10_000, min_conf, &sp).await?;
            sp.finish_and_clear();

            let bal = wallet.balance().await?;
            eprintln!("{} Confirmed — spendable: {}",
                style("✓").green().bold(),
                style(format_zat(bal.spendable_zat)).cyan()
            );

            wallet.close_runtime().await?;
            println!("{txid}");
        }

        "accounts" => {
            let subcmd = pos.get(1).copied().unwrap_or("list");
            match subcmd {
                "list" => {
                    let wallet = open_existing(&data_dir, &lwd, network, 0).await?;
                    let accounts = wallet.accounts_list().await?;
                    eprintln!("{}", style("Accounts").bold());
                    for (idx, addr) in &accounts {
                        let marker = if *idx == account_index { style("*").green().bold().to_string() } else { " ".to_string() };
                        eprintln!("  {marker} [{}] {}", idx, style(addr).cyan());
                    }
                    eprintln!();
                    eprintln!("  Use --account <n> to select an account.");
                }
                _ => {
                    eprintln!("{}", style("zimppy-wallet accounts").bold());
                    eprintln!();
                    eprintln!("{}:", style("Subcommands").underlined());
                    eprintln!("  {}   List all accounts in this wallet", style("list").green());
                    eprintln!();
                    eprintln!("  To create a wallet with multiple accounts:");
                    eprintln!("    zimppy-wallet init \"<seed>\" <birthday> --accounts <n>");
                }
            }
        }

        _ => {
            eprintln!("{}", style("zimppy-wallet").bold());
            eprintln!("Native Zcash wallet CLI");
            eprintln!();
            eprintln!("{}:", style("Commands").underlined());
            eprintln!("  {} <seed> <birthday> [--accounts N]  Create/restore wallet", style("init").green());
            eprintln!("  {}                                    Sync with blockchain", style("sync").green());
            eprintln!("  {}                                 Show unified address", style("address").green());
            eprintln!("  {}                                 Show balance", style("balance").green());
            eprintln!("  {} <addr> <zat> [memo]               Send ZEC", style("send").green());
            eprintln!("  {} list                              List all accounts", style("accounts").green());
            eprintln!();
            eprintln!("{}:", style("Flags").underlined());
            eprintln!("  {} <url>          Lightwalletd endpoint (overrides env)", style("--rpc").yellow());
            eprintln!("  {} <n>        Account index to use (default: 0)", style("--account").yellow());
            eprintln!("  {}                 Show debug logs on stderr", style("--debug").yellow());
            eprintln!();
            eprintln!("{}:", style("Environment").underlined());
            eprintln!("  ZIMPPY_WALLET_DIR           Data directory");
            eprintln!("  ZIMPPY_LWD_ENDPOINT         Lightwalletd URL");
            eprintln!("  ZIMPPY_NETWORK              mainnet | testnet");
            eprintln!("  ZIMPPY_MIN_CONFIRMATIONS    Min confirmations (default: 3)");
        }
    }

    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────

async fn wait_for_maturity(
    wallet: &mut ZimppyWallet,
    needed: u64,
    min_conf: u32,
    sp: &ProgressBar,
) -> Result<(), WalletError> {
    let max_polls = std::cmp::max(
        20usize,
        ((min_conf as u64) * BLOCK_TIME_SECS * 2 / 15) as usize,
    );
    let mut stale_count = 0u32;
    let stale_limit = std::cmp::max(min_conf * 8, 24);
    let started = std::time::Instant::now();
    let est_secs = min_conf as u64 * 75;

    let mut last_bal: Option<zimppy_wallet::WalletBalance> = None;
    for _attempt in 1..=max_polls {
        for _ in 0..POLL_INTERVAL.as_secs() {
            let elapsed = started.elapsed().as_secs();
            let remaining = est_secs.saturating_sub(elapsed);
            let status = match &last_bal {
                Some(b) if b.pending_zat > 0 => format!(
                    "Confirming... {}s elapsed, ~{}s remaining", elapsed, remaining
                ),
                _ => format!("Waiting for maturity... {}s elapsed", elapsed),
            };
            sp.set_message(status);
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        wallet.sync().await?;
        let bal = wallet.balance().await?;

        if bal.spendable_zat >= needed {
            return Ok(());
        }
        if bal.pending_zat == 0 && bal.total_zat < needed {
            return Ok(());
        }
        if bal.pending_zat == 0 && bal.spendable_zat < needed {
            stale_count += 1;
            if stale_count > stale_limit {
                return Ok(());
            }
        } else {
            stale_count = 0;
        }
        last_bal = Some(bal);
    }
    Ok(())
}

async fn open_existing(
    data_dir: &PathBuf,
    lwd: &str,
    network: NetworkType,
    account_index: u32,
) -> Result<ZimppyWallet, WalletError> {
    ZimppyWallet::open(WalletConfig {
        data_dir: data_dir.clone(),
        lwd_endpoint: lwd.to_string(),
        network,
        seed_phrase: None,
        birthday_height: None,
        account_index,
        num_accounts: 1,
    })
    .await
}
