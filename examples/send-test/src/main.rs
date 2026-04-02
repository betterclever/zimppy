use zimppy_wallet::{WalletConfig, ZimppyWallet};
use zcash_protocol::consensus::NetworkType;

const SEED: &str = "fossil afraid giraffe curious glad sadness short wise pulse slot shove rigid cactus razor fall mimic spatial title funny poet lesson manage lava caught";
const BIRTHDAY: u32 = 3906900;
const LWD: &str = "https://testnet.zec.rocks";

fn usage() {
    eprintln!("Usage:");
    eprintln!("  send-test restore              Create/restore wallet from seed");
    eprintln!("  send-test balance              Sync and show balance");
    eprintln!("  send-test send <addr> <zats>   Sync, send, wait for confirmation, sync again");
    eprintln!("  send-test send3 <addr> <zats>  3 consecutive sends with confirmation waits");
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("help");

    let data_dir = dirs::home_dir()
        .expect("home dir")
        .join(".zimppy/wallets/send-test");
    std::fs::create_dir_all(&data_dir).expect("mkdir");

    match cmd {
        "restore" => {
            if data_dir.join("zingo-wallet.dat").exists() {
                eprintln!("Wallet already exists at {}", data_dir.display());
                eprintln!("Delete it first if you want a fresh restore.");
                return;
            }
            let config = WalletConfig {
                data_dir,
                lwd_endpoint: LWD.to_string(),
                network: NetworkType::Test,
                seed_phrase: Some(SEED.to_string()),
                birthday_height: Some(BIRTHDAY),
                account_index: 0,
                num_accounts: 1,
                passphrase: None,
            };
            eprintln!("Restoring wallet from seed...");
            let mut wallet = ZimppyWallet::open(config).await.expect("restore");
            eprintln!("Syncing...");
            wallet.sync().await.expect("sync");
            let bal = wallet.balance().await.expect("balance");
            eprintln!("Balance: spendable={} pending={} total={}", bal.spendable_zat, bal.pending_zat, bal.total_zat);
            wallet.close_runtime().await.expect("close");
            eprintln!("Done.");
        }
        "balance" => {
            let config = WalletConfig {
                data_dir,
                lwd_endpoint: LWD.to_string(),
                network: NetworkType::Test,
                seed_phrase: None,
                birthday_height: None,
                account_index: 0,
                num_accounts: 1,
                passphrase: None,
            };
            let mut wallet = ZimppyWallet::open(config).await.expect("open");
            eprintln!("Syncing...");
            wallet.sync().await.expect("sync");
            let bal = wallet.balance().await.expect("balance");
            eprintln!("Balance: spendable={} pending={} total={}", bal.spendable_zat, bal.pending_zat, bal.total_zat);
            wallet.close_runtime().await.expect("close");
        }
        "send" => {
            let addr = args.get(2).expect("Usage: send-test send <addr> <zats>");
            let amount: u64 = args.get(3).expect("Usage: send-test send <addr> <zats>")
                .parse().expect("amount must be a number");

            let config = WalletConfig {
                data_dir,
                lwd_endpoint: LWD.to_string(),
                network: NetworkType::Test,
                seed_phrase: None,
                birthday_height: None,
                account_index: 0,
                num_accounts: 1,
                passphrase: None,
            };

            // Open existing wallet
            eprintln!("Opening wallet...");
            let mut wallet = ZimppyWallet::open(config).await.expect("open");

            // Sync
            eprintln!("Syncing...");
            wallet.sync().await.expect("sync");
            let bal = wallet.balance().await.expect("balance");
            eprintln!("Balance: spendable={} pending={} total={}", bal.spendable_zat, bal.pending_zat, bal.total_zat);

            // Send
            eprintln!("Sending {} zats to {}...", amount, &addr[..25]);
            match wallet.send(addr, amount, None).await {
                Ok(txid) => {
                    eprintln!("SENT: txid={txid}");

                    // Wait for confirmation — poll every 15s, up to 5 min
                    eprintln!("Waiting for confirmation...");
                    for attempt in 1..=20 {
                        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                        eprint!("  [{attempt}/20] syncing... ");

                        wallet.sync().await.expect("sync");
                        let bal = wallet.balance().await.expect("balance");
                        eprintln!("spendable={} pending={}", bal.spendable_zat, bal.pending_zat);

                        // If pending is 0 and spendable is > 0, the change is confirmed
                        if bal.pending_zat == 0 && bal.spendable_zat > 0 {
                            eprintln!("Confirmed!");
                            break;
                        }
                    }

                    let bal = wallet.balance().await.expect("balance");
                    eprintln!("Final balance: spendable={} pending={} total={}", bal.spendable_zat, bal.pending_zat, bal.total_zat);
                }
                Err(e) => {
                    eprintln!("SEND FAILED: {e}");
                }
            }

            wallet.close_runtime().await.expect("close");
            eprintln!("Done.");
        }
        "send3" => {
            let addr = args.get(2).expect("Usage: send-test send3 <addr> <zats>");
            let amount: u64 = args.get(3).expect("Usage: send-test send3 <addr> <zats>")
                .parse().expect("amount must be a number");

            let config = WalletConfig {
                data_dir,
                lwd_endpoint: LWD.to_string(),
                network: NetworkType::Test,
                seed_phrase: None,
                birthday_height: None,
                account_index: 0,
                num_accounts: 1,
                passphrase: None,
            };

            eprintln!("Opening wallet...");
            let mut wallet = ZimppyWallet::open(config).await.expect("open");

            eprintln!("Initial sync...");
            wallet.sync().await.expect("sync");
            let bal = wallet.balance().await.expect("balance");
            eprintln!("Balance: spendable={} pending={} total={}", bal.spendable_zat, bal.pending_zat, bal.total_zat);

            for tx_num in 1..=3 {
                eprintln!("\n========== TX {tx_num}/3 ==========");
                eprintln!("Sending {} zats to {}...", amount, &addr[..25]);
                match wallet.send(addr, amount, None).await {
                    Ok(txid) => {
                        eprintln!("SENT #{tx_num}: txid={txid}");

                        eprintln!("Waiting for confirmation...");
                        let mut confirmed = false;
                        for attempt in 1..=24 {
                            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                            eprint!("  [{attempt}/24] syncing... ");

                            wallet.sync().await.expect("sync");
                            let bal = wallet.balance().await.expect("balance");
                            eprintln!("spendable={} pending={}", bal.spendable_zat, bal.pending_zat);

                            if bal.pending_zat == 0 && bal.spendable_zat > 0 {
                                eprintln!("TX #{tx_num} confirmed!");
                                confirmed = true;
                                break;
                            }
                        }
                        if !confirmed {
                            eprintln!("TX #{tx_num} NOT confirmed after 6 min, aborting.");
                            break;
                        }
                    }
                    Err(e) => {
                        eprintln!("TX #{tx_num} SEND FAILED: {e}");
                        break;
                    }
                }
            }

            let bal = wallet.balance().await.expect("balance");
            eprintln!("\n========== FINAL ==========");
            eprintln!("Balance: spendable={} pending={} total={}", bal.spendable_zat, bal.pending_zat, bal.total_zat);
            wallet.close_runtime().await.expect("close");
            eprintln!("Done.");
        }
        _ => usage(),
    }
}
