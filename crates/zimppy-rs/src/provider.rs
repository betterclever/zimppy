use std::process::Command;
use std::time::Duration;

use mpp::protocol::core::{PaymentChallenge, PaymentCredential, PaymentPayload};
use mpp::client::PaymentProvider;
use mpp::error::MppError;

use zimppy_core::rpc::ZebradRpc;

/// Zcash payment provider for the mpp-rs client.
///
/// When an agent receives a 402, this provider:
/// 1. Parses the challenge (recipient, amount, memo)
/// 2. Sends a real Orchard shielded tx via zcash-devtool
/// 3. Waits for on-chain confirmation
/// 4. Returns a credential with the txid
#[derive(Clone)]
pub struct ZcashPaymentProvider {
    wallet_dir: String,
    identity_file: String,
    lightwalletd_server: String,
    rpc_endpoint: String,
    /// Max seconds to wait for tx confirmation
    confirmation_timeout: u64,
}

impl ZcashPaymentProvider {
    pub fn new(
        wallet_dir: &str,
        identity_file: &str,
        lightwalletd_server: &str,
        rpc_endpoint: &str,
    ) -> Self {
        Self {
            wallet_dir: wallet_dir.to_string(),
            identity_file: identity_file.to_string(),
            lightwalletd_server: lightwalletd_server.to_string(),
            rpc_endpoint: rpc_endpoint.to_string(),
            confirmation_timeout: 300,
        }
    }

    pub fn with_timeout(mut self, seconds: u64) -> Self {
        self.confirmation_timeout = seconds;
        self
    }

    /// Send ZEC via zcash-devtool and return the txid
    fn send_payment(&self, address: &str, amount_zat: u64, memo: &str) -> Result<String, MppError> {
        eprintln!("[ZcashProvider] Syncing wallet...");
        let sync = Command::new("zcash-devtool")
            .args(["wallet", "-w", &self.wallet_dir, "sync",
                   "--server", &self.lightwalletd_server,
                   "--connection", "direct"])
            .output()
            .map_err(|e| MppError::InvalidConfig(format!("failed to run zcash-devtool sync: {e}")))?;

        if !sync.status.success() {
            let stderr = String::from_utf8_lossy(&sync.stderr);
            return Err(MppError::InvalidConfig(format!("wallet sync failed: {stderr}")));
        }

        eprintln!("[ZcashProvider] Sending {} zat to {}...", amount_zat, &address[..20]);
        eprintln!("[ZcashProvider] Memo: {memo}");

        let send = Command::new("zcash-devtool")
            .args(["wallet", "-w", &self.wallet_dir, "send",
                   "-i", &self.identity_file,
                   "--server", &self.lightwalletd_server,
                   "--connection", "direct",
                   "--address", address,
                   "--value", &amount_zat.to_string(),
                   "--memo", memo])
            .output()
            .map_err(|e| MppError::InvalidConfig(format!("failed to run zcash-devtool send: {e}")))?;

        let stdout = String::from_utf8_lossy(&send.stdout);
        let stderr = String::from_utf8_lossy(&send.stderr);

        if !send.status.success() {
            return Err(MppError::InvalidConfig(format!("send failed: {stderr}")));
        }

        // Extract txid — it's the 64-char hex string in stdout
        let txid = stdout
            .lines()
            .find(|line| line.len() == 64 && line.chars().all(|c| c.is_ascii_hexdigit()))
            .ok_or_else(|| MppError::InvalidConfig(format!("no txid in output: {stdout}")))?
            .to_string();

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

            match rpc.get_transaction_verbose(txid).await {
                Ok(tx) => {
                    let confs = tx.confirmations.unwrap_or(0);
                    if confs > 0 {
                        eprintln!("[ZcashProvider] Confirmed! {} confirmations", confs);
                        return Ok(confs);
                    }
                }
                Err(_) => {} // tx not found yet, keep polling
            }

            tokio::time::sleep(Duration::from_secs(15)).await;
            eprint!(".");
        }
    }
}

impl PaymentProvider for ZcashPaymentProvider {
    fn supports(&self, method: &str, intent: &str) -> bool {
        method == "zcash" && intent == "charge"
    }

    async fn pay(&self, challenge: &PaymentChallenge) -> Result<PaymentCredential, MppError> {
        // Parse challenge request to get recipient, amount, memo
        let request: serde_json::Value = challenge.request.decode()
            .map_err(|e| MppError::InvalidConfig(format!("failed to decode challenge request: {e}")))?;

        let recipient = request["recipient"]
            .as_str()
            .ok_or_else(|| MppError::InvalidConfig("missing recipient in challenge".to_string()))?;
        let amount_str = request["amount"]
            .as_str()
            .ok_or_else(|| MppError::InvalidConfig("missing amount in challenge".to_string()))?;
        let memo = request["memo"]
            .as_str()
            .ok_or_else(|| MppError::InvalidConfig("missing memo in challenge".to_string()))?;
        let challenge_id = request["challengeId"]
            .as_str()
            .ok_or_else(|| MppError::InvalidConfig("missing challengeId in challenge".to_string()))?;

        let amount_zat: u64 = amount_str.parse()
            .map_err(|_| MppError::InvalidConfig("invalid amount".to_string()))?;

        eprintln!("[ZcashProvider] Received 402 challenge:");
        eprintln!("[ZcashProvider]   recipient: {}", &recipient[..20]);
        eprintln!("[ZcashProvider]   amount: {} zat", amount_zat);
        eprintln!("[ZcashProvider]   memo: {memo}");

        // Send real ZEC
        let txid = self.send_payment(recipient, amount_zat, memo)?;

        // Wait for confirmation
        self.wait_for_confirmation(&txid).await?;

        // Build credential
        let echo = challenge.to_echo();
        let payload = PaymentPayload::hash(&txid);
        let mut credential = PaymentCredential::new(echo, payload);

        // Also inject our challengeId into the payload for the server
        // The server needs both txid and challengeId
        // Override the raw payload to include both
        credential.payload = serde_json::json!({
            "type": "hash",
            "hash": txid,
            "challengeId": challenge_id,
        });

        eprintln!("[ZcashProvider] Credential ready with txid {}", &txid[..16]);
        Ok(credential)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_zcash_charge() {
        let provider = ZcashPaymentProvider::new("/tmp/w", "/tmp/i", "server", "rpc");
        assert!(provider.supports("zcash", "charge"));
        assert!(!provider.supports("tempo", "charge"));
        assert!(!provider.supports("zcash", "session"));
    }
}
