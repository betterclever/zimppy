//! # Zimppy Fortune Teller — Rust Client
//!
//! Demonstrates the `send_with_payment` API for automatic 402 handling
//! with Zcash shielded payments.
//!
//! ```bash
//! # Start the server first:
//! cargo run --release --bin zimppy-rust-server
//!
//! # Then run the client:
//! cargo run --release --bin zimppy-rust-client
//! ```

use mpp::client::Fetch;
use mpp::parse_receipt;
use reqwest::Client;
use zimppy_rs::ZcashPaymentProvider;
use zimppy_wallet::WalletConfig;

#[tokio::main]
async fn main() {
    let url = std::env::var("FORTUNE_URL")
        .unwrap_or_else(|_| "http://localhost:3180/api/fortune".to_string());

    let rpc_endpoint = std::env::var("ZEBRAD_RPC_ENDPOINT")
        .unwrap_or_else(|_| "https://zcash-testnet-zebrad.gateway.tatum.io".to_string());

    let wallet_dir = std::env::var("ZIMPPY_WALLET_DIR").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        format!("{home}/.zimppy/wallets/default")
    });

    let lwd_endpoint = std::env::var("ZIMPPY_LWD_ENDPOINT")
        .unwrap_or_else(|_| "https://testnet.zec.rocks".to_string());

    eprintln!("=== zimppy fortune client ===");
    eprintln!("  url: {url}");
    eprintln!("  wallet: {wallet_dir}");
    eprintln!();

    let provider = ZcashPaymentProvider::new(
        WalletConfig {
            data_dir: wallet_dir.into(),
            lwd_endpoint,
            network: zcash_protocol::consensus::NetworkType::Test,
            seed_phrase: None,
            birthday_height: None,
        },
        &rpc_endpoint,
    );

    let client = Client::new();

    eprintln!("Fetching {url} ...");

    let resp = client
        .get(&url)
        .send_with_payment(&provider)
        .await
        .expect("request failed");

    eprintln!("Status: {}", resp.status());

    if let Some(receipt_hdr) = resp.headers().get("payment-receipt") {
        if let Ok(receipt_str) = receipt_hdr.to_str() {
            if let Ok(receipt) = parse_receipt(receipt_str) {
                eprintln!("Receipt: ref={}", receipt.reference);
            }
        }
    }

    let body = resp.text().await.expect("failed to read body");

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
        if let Some(fortune) = json.get("fortune").and_then(|v| v.as_str()) {
            eprintln!();
            eprintln!("Fortune: {fortune}");
        } else {
            eprintln!("Response: {json}");
        }
    } else {
        eprintln!("Response: {body}");
    }
}
