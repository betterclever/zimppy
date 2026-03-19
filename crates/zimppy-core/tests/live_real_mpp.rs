#![cfg(feature = "shielded")]

use zimppy_core::rpc::ZebradRpc;
use zimppy_core::shielded::{verify_shielded, ShieldedVerifyRequest};
use zimppy_core::replay::ConsumedTxids;

// Real transaction: sent from zcash-devtool wallet to keygen wallet
// with memo "zimppy:challenge-real-001"
const REAL_MPP_TXID: &str = "257645788655583e4da766a59d0428faeb7a7fcd0cf50531542d2deb032651a2";

// Keygen wallet's Orchard IVK (the server's viewing key)
const SERVER_IVK: &str = "f803b8959eb4529227a0a3806db7302e8f7f77a0fa2898201f8c15862bafc71f84e798b8b3af27c6b6365f4b50d581396a7c8ca20ef188982cd4e28f47349c0a";

#[tokio::test]
async fn verify_real_mpp_payment_with_memo() {
    let rpc = ZebradRpc::new("https://zcash-testnet-zebrad.gateway.tatum.io");
    let consumed = ConsumedTxids::new();
    
    let result = verify_shielded(&rpc, &ShieldedVerifyRequest {
        txid: REAL_MPP_TXID.to_string(),
        ivk_bytes_hex: SERVER_IVK.to_string(),
        expected_challenge_id: "challenge-real-001".to_string(),
        expected_amount_zat: 10000,
    }, &consumed).await;
    
    match &result {
        Ok(r) => {
            println!("=== REAL MPP SHIELDED VERIFICATION ===");
            println!("  txid: {}", r.txid);
            println!("  verified: {}", r.verified);
            println!("  outputs decrypted: {}", r.outputs_decrypted);
            println!("  observed amount: {} zat ({} ZEC)", r.observed_amount_zat, r.observed_amount_zat as f64 / 100_000_000.0);
            println!("  memo matched: {}", r.memo_matched);
            
            assert!(r.outputs_decrypted > 0, "should decrypt at least one output");
            assert!(r.observed_amount_zat >= 10000, "should have at least 10000 zat");
            assert!(r.memo_matched, "memo should contain challenge-real-001");
            assert!(r.verified, "payment should be fully verified");
            
            println!("  === ALL CHECKS PASSED ===");
        }
        Err(e) => {
            panic!("verification failed: {}", e);
        }
    }
}
