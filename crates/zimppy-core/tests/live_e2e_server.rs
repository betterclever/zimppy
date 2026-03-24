#![cfg(feature = "shielded")]

use zimppy_core::replay::ConsumedTxids;
use zimppy_core::rpc::ZebradRpc;
use zimppy_core::shielded::{verify_shielded, ShieldedVerifyRequest};

// Real tx sent from client wallet to server wallet
const E2E_TXID: &str = "764f92e359c72f45457fdb1fa4b204108aef60a9c18253dd9bf604a3d12eff80";

// Server wallet's Orchard IVK (from config/server-wallet.json)
const SERVER_IVK: &str = "803af23f313532b979a0f2a52f575ef4b5f76290c731ab8837c876cb8034b13a0be8503539505ea7e5c169bce9df4eac0c496c388c05d9326c70e0864f6ac806";

#[tokio::test]
async fn e2e_server_verifies_client_payment() {
    let rpc = ZebradRpc::new("https://zcash-testnet-zebrad.gateway.tatum.io");
    let consumed = ConsumedTxids::new();

    println!("=== E2E: Server Verifying Client Payment ===");
    println!("  txid: {E2E_TXID}");
    println!("  server IVK: {SERVER_IVK}");
    println!("  expected challenge: challenge-e2e-test-001");
    println!("  expected amount: 42000 zat");
    println!();

    let result = verify_shielded(
        &rpc,
        &ShieldedVerifyRequest {
            txid: E2E_TXID.to_string(),
            ivk_bytes_hex: SERVER_IVK.to_string(),
            expected_challenge_id: "challenge-e2e-test-001".to_string(),
            expected_amount_zat: 42000,
        },
        &consumed,
    )
    .await;

    match &result {
        Ok(r) => {
            println!("  Result:");
            println!("    verified: {}", r.verified);
            println!("    outputs_decrypted: {}", r.outputs_decrypted);
            println!("    observed_amount: {} zat", r.observed_amount_zat);
            println!("    memo_matched: {}", r.memo_matched);
            println!();

            assert!(r.outputs_decrypted > 0, "should decrypt server's output");
            assert_eq!(r.observed_amount_zat, 42000, "should be 42000 zat");
            assert!(r.memo_matched, "memo should contain challenge-e2e-test-001");
            assert!(r.verified, "payment should be fully verified");

            println!("  === E2E TEST PASSED ===");
            println!("  Server successfully verified a real shielded payment:");
            println!("  - Decrypted Orchard output with server's viewing key");
            println!("  - Confirmed 42000 zat received");
            println!("  - Confirmed memo contains the challenge ID");
            println!("  - All on real Zcash testnet!");
        }
        Err(e) => panic!("E2E verification failed: {e}"),
    }
}
