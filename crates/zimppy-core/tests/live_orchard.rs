#![cfg(feature = "shielded")]

use zimppy_core::replay::ConsumedTxids;
use zimppy_core::rpc::ZebradRpc;
use zimppy_core::shielded::{verify_shielded, ShieldedVerifyRequest};

const IVK: &str = "f803b8959eb4529227a0a3806db7302e8f7f77a0fa2898201f8c15862bafc71f84e798b8b3af27c6b6365f4b50d581396a7c8ca20ef188982cd4e28f47349c0a";

async fn try_decrypt(rpc: &ZebradRpc, txid: &str) -> String {
    let consumed = ConsumedTxids::new();
    let result = verify_shielded(
        rpc,
        &ShieldedVerifyRequest {
            txid: txid.to_string(),
            ivk_bytes_hex: IVK.to_string(),
            expected_challenge_id: String::new(),
            expected_amount_zat: 0,
        },
        &consumed,
    )
    .await;

    match result {
        Ok(r) => format!(
            "decrypted={} amount={} memo={}",
            r.outputs_decrypted, r.observed_amount_zat, r.memo_matched
        ),
        Err(e) => format!("error: {e}"),
    }
}

#[tokio::test]
async fn scan_recent_txs_for_faucet() {
    let rpc = ZebradRpc::new("https://zcash-testnet-zebrad.gateway.tatum.io");

    let txids = [
        "5d9e53b3aa6129f78b2ea575a5999fad3902ce187a75dca8287276c371b7026e", // block 3906823
        "9126607f70a1f5a1083fe7195df6c4e01c68f08710d9081205f0b7355d5b233b", // block 3906699
        "510da4028c9f8511f3016167a7910f036945de5c54202c76d142b35b94cd60a7", // block 3906663
        "cc29965f905244adeaf710092743bc84e9861a02e42012175ee7fae54957f327", // block 3906649
        "c5c418e9de273ba26d4574ffc7e6d1421a5fb47cdef5bfae9e5ff3a1ba59ad31", // block 3906648
        "5217dffdbcba639483bd332b30a59dcbb282138d8d2555657cb50db4d50ca484", // block 3906639
    ];

    for txid in &txids {
        let result = try_decrypt(&rpc, txid).await;
        println!("tx {}: {}", &txid[..16], result);
        // Rate limit - Tatum has 5 req/min
        tokio::time::sleep(std::time::Duration::from_secs(13)).await;
    }
}
