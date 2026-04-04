use std::time::Duration;

use mpp::client::PaymentProvider;
use mpp::error::MppError;
use mpp::protocol::core::{PaymentChallenge, PaymentCredential, PaymentPayload};

use zimppy_core::rpc::ZebradRpc;
use zimppy_wallet::{WalletConfig, ZimppyWallet};

/// Zcash payment provider for the mpp-rs client.
///
/// When an agent receives a 402, this provider:
/// 1. Parses the challenge (recipient, amount, memo)
/// 2. Sends a real Orchard shielded tx via native wallet
/// 3. Waits for on-chain confirmation
/// 4. Returns a credential with the txid
#[derive(Clone)]
pub struct ZcashPaymentProvider {
    wallet_config: WalletConfig,
    rpc_endpoint: String,
    /// Max seconds to wait for tx confirmation
    confirmation_timeout: u64,
}

impl ZcashPaymentProvider {
    pub fn new(wallet_config: WalletConfig, rpc_endpoint: &str) -> Self {
        Self {
            wallet_config,
            rpc_endpoint: rpc_endpoint.to_string(),
            confirmation_timeout: 300,
        }
    }

    pub fn with_timeout(mut self, seconds: u64) -> Self {
        self.confirmation_timeout = seconds;
        self
    }

    /// Send ZEC via native wallet and return the txid
    async fn send_payment(
        &self,
        address: &str,
        amount_zat: u64,
        memo: &str,
    ) -> Result<String, MppError> {
        eprintln!("[ZcashProvider] Opening wallet and syncing...");
        let mut wallet = ZimppyWallet::open(WalletConfig {
            data_dir: self.wallet_config.data_dir.clone(),
            lwd_endpoint: self.wallet_config.lwd_endpoint.clone(),
            network: self.wallet_config.network,
            seed_phrase: self.wallet_config.seed_phrase.clone(),
            birthday_height: self.wallet_config.birthday_height,
            account_index: self.wallet_config.account_index,
            num_accounts: self.wallet_config.num_accounts,
            passphrase: self.wallet_config.passphrase.clone(),
        })
        .await
        .map_err(|e| MppError::InvalidConfig(format!("wallet open failed: {e}")))?;

        wallet
            .sync()
            .await
            .map_err(|e| MppError::InvalidConfig(format!("wallet sync failed: {e}")))?;

        eprintln!(
            "[ZcashProvider] Sending {} zat to {}...",
            amount_zat,
            &address[..20.min(address.len())]
        );
        eprintln!("[ZcashProvider] Memo: {memo}");

        let txid = wallet
            .send(address, amount_zat, if memo.is_empty() { None } else { Some(memo) })
            .await
            .map_err(|e| MppError::InvalidConfig(format!("send failed: {e}")))?;

        eprintln!("[ZcashProvider] Broadcast txid: {txid}");
        Ok(txid)
    }

    /// Wait for a transaction to get at least 1 confirmation
    async fn wait_for_confirmation(&self, txid: &str) -> Result<u32, MppError> {
        let rpc = ZebradRpc::new(&self.rpc_endpoint);
        let start = std::time::Instant::now();

        eprintln!("[ZcashProvider] Waiting for confirmation...");

        loop {
            if start.elapsed() > Duration::from_secs(self.confirmation_timeout) {
                return Err(MppError::InvalidConfig("confirmation timeout".to_string()));
            }

            if let Ok(tx) = rpc.get_transaction_verbose(txid).await {
                let confs = tx.confirmations.unwrap_or(0);
                if confs > 0 {
                    eprintln!("[ZcashProvider] Confirmed! {} confirmations", confs);
                    return Ok(confs);
                }
            }

            tokio::time::sleep(Duration::from_secs(15)).await;
            eprint!(".");
        }
    }
}

impl PaymentProvider for ZcashPaymentProvider {
    fn supports(&self, method: &str, intent: &str) -> bool {
        (method == "zcash" || method == "zcash-transparent") && intent == "charge"
    }

    async fn pay(&self, challenge: &PaymentChallenge) -> Result<PaymentCredential, MppError> {
        // Parse challenge request to get recipient and amount
        let request: serde_json::Value = challenge.request.decode().map_err(|e| {
            MppError::InvalidConfig(format!("failed to decode challenge request: {e}"))
        })?;

        let recipient = request["recipient"]
            .as_str()
            .ok_or_else(|| MppError::InvalidConfig("missing recipient in challenge".to_string()))?;
        let amount_str = request["amount"]
            .as_str()
            .ok_or_else(|| MppError::InvalidConfig("missing amount in challenge".to_string()))?;

        let amount_zat: u64 = amount_str
            .parse()
            .map_err(|_| MppError::InvalidConfig("invalid amount".to_string()))?;

        // Memo is present for shielded payments, absent for transparent
        let memo_opt = request["methodDetails"]["memo"]
            .as_str()
            .map(|m| m.replace("{id}", &challenge.id));
        let memo = memo_opt.as_deref().unwrap_or("");

        eprintln!("[ZcashProvider] Received 402 challenge:");
        eprintln!(
            "[ZcashProvider]   recipient: {}",
            &recipient[..20.min(recipient.len())]
        );
        eprintln!("[ZcashProvider]   amount: {} zat", amount_zat);
        if !memo.is_empty() {
            eprintln!("[ZcashProvider]   memo: {}", memo);
        }

        // Send ZEC (works for both T-addresses and shielded addresses)
        let txid = self.send_payment(recipient, amount_zat, memo).await?;

        // Wait for on-chain confirmation
        self.wait_for_confirmation(&txid).await?;

        // Build credential — include outputIndex for transparent, omit for shielded
        let echo = challenge.to_echo();
        let payload = PaymentPayload::hash(&txid);
        let mut credential = PaymentCredential::new(echo, payload);

        credential.payload = if challenge.method.as_str() == "zcash-transparent" {
            // Transparent: server verifies via output index (always 0 for single-recipient sends)
            serde_json::json!({ "txid": txid, "outputIndex": 0 })
        } else {
            // Shielded: server verifies via IVK, no outputIndex needed
            serde_json::json!({ "txid": txid })
        };

        eprintln!("[ZcashProvider] Credential ready with txid {}", &txid[..16.min(txid.len())]);
        Ok(credential)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use zcash_protocol::consensus::NetworkType;

    #[test]
    fn supports_zcash_charge() {
        let provider = ZcashPaymentProvider::new(
            WalletConfig {
                data_dir: PathBuf::from("/tmp/w"),
                lwd_endpoint: "https://testnet.zec.rocks".to_string(),
                network: NetworkType::Test,
                seed_phrase: None,
                birthday_height: None,
                account_index: 0,
                num_accounts: 1,
                passphrase: None,
            },
            "https://rpc.example.com",
        );
        assert!(provider.supports("zcash", "charge"));
        assert!(!provider.supports("tempo", "charge"));
        assert!(!provider.supports("zcash", "session"));
    }

    #[test]
    fn supports_zcash_transparent_charge() {
        let provider = ZcashPaymentProvider::new(
            WalletConfig {
                data_dir: PathBuf::from("/tmp/w"),
                lwd_endpoint: "https://testnet.zec.rocks".to_string(),
                network: NetworkType::Test,
                seed_phrase: None,
                birthday_height: None,
                account_index: 0,
                num_accounts: 1,
                passphrase: None,
            },
            "https://rpc.example.com",
        );
        assert!(provider.supports("zcash-transparent", "charge"));
        assert!(!provider.supports("zcash-transparent", "session"));
        // shielded still works
        assert!(provider.supports("zcash", "charge"));
    }
}
