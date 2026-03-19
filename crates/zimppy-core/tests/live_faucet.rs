#![cfg(feature = "shielded")]

use zimppy_core::rpc::ZebradRpc;
use zimppy_core::shielded::{verify_shielded, ShieldedVerifyRequest};
use zimppy_core::replay::ConsumedTxids;

const FAUCET_TXID: &str = "0ea14d73764748207a48fd3f76e7ae34a3717a576db1acb5f8e5020391397cb3";

// Orchard IVK from keygen wallet
const KEYGEN_IVK: &str = "f803b8959eb4529227a0a3806db7302e8f7f77a0fa2898201f8c15862bafc71f84e798b8b3af27c6b6365f4b50d581396a7c8ca20ef188982cd4e28f47349c0a";

#[tokio::test]
async fn decrypt_faucet_tx() {
    let rpc = ZebradRpc::new("https://zcash-testnet-zebrad.gateway.tatum.io");
    
    // Try keygen IVK
    let consumed = ConsumedTxids::new();
    let result = verify_shielded(&rpc, &ShieldedVerifyRequest {
        txid: FAUCET_TXID.to_string(),
        ivk_bytes_hex: KEYGEN_IVK.to_string(),
        expected_challenge_id: String::new(),
        expected_amount_zat: 0,
    }, &consumed).await;
    
    match &result {
        Ok(r) => println!("Keygen IVK: decrypted={} amount={} zat ({} ZEC)", 
            r.outputs_decrypted, r.observed_amount_zat, 
            r.observed_amount_zat as f64 / 100_000_000.0),
        Err(e) => println!("Keygen IVK error: {e}"),
    }
    
    // Check if we decrypted anything
    if let Ok(r) = &result {
        if r.outputs_decrypted > 0 {
            println!("SUCCESS! Found {} TAZ in faucet tx!", r.observed_amount_zat as f64 / 100_000_000.0);
            return;
        }
    }
    
    println!("Keygen IVK didn't match — tx may be for a different wallet");
}
