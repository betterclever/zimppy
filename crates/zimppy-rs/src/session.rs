use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use zimppy_core::replay::ConsumedTxids;
use zimppy_core::rpc::ZebradRpc;
use zimppy_core::shielded::{self, ShieldedVerifyRequest};

use mpp::protocol::core::{PaymentCredential, Receipt};
use mpp::protocol::intents::SessionRequest;
use mpp::protocol::traits::{SessionMethod, VerificationError};

/// Session state stored per active session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub session_id: String,
    pub bearer_hash: String,
    pub deposit_amount_zat: u64,
    pub spent_zat: u64,
    pub refund_address: String,
    pub network: String,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    Active,
    Closing,
    Closed,
}

/// Credential payload for session actions.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum SessionPayload {
    Open {
        #[serde(rename = "depositTxid")]
        deposit_txid: String,
        #[serde(rename = "refundAddress")]
        refund_address: String,
        #[serde(rename = "bearerSecret", default)]
        bearer_secret: Option<String>,
    },
    Bearer {
        #[serde(rename = "sessionId")]
        session_id: String,
        bearer: String,
    },
    TopUp {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "topUpTxid")]
        top_up_txid: String,
    },
    Close {
        #[serde(rename = "sessionId")]
        session_id: String,
        bearer: String,
    },
}

/// Re-export wallet config for refund sending.
pub use zimppy_wallet::WalletConfig as RefundConfig;

/// Zcash session method — manages prepaid balance sessions.
#[derive(Clone)]
pub struct ZcashSessionMethod {
    rpc: ZebradRpc,
    orchard_ivk: String,
    network: String,
    consumed: ConsumedTxids,
    sessions: Arc<Mutex<HashMap<String, SessionState>>>,
    refund_config: Option<RefundConfig>,
    sessions_dir: Option<PathBuf>,
    /// Cache of recent verify results for the `respond()` trait hook.
    last_verify_results: Arc<Mutex<HashMap<String, SessionVerifyResult>>>,
}

impl ZcashSessionMethod {
    pub fn new(rpc_endpoint: &str, orchard_ivk: &str) -> Self {
        Self {
            rpc: ZebradRpc::new(rpc_endpoint),
            orchard_ivk: orchard_ivk.to_string(),
            network: "testnet".to_string(),
            consumed: ConsumedTxids::new(),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            refund_config: None,
            sessions_dir: None,
            last_verify_results: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_network(mut self, network: &str) -> Self {
        self.network = network.to_string();
        self
    }

    pub fn with_refund_config(mut self, config: RefundConfig) -> Self {
        self.refund_config = Some(config);
        self
    }

    /// Enable file-backed session persistence. Loads all existing session JSON files
    /// from `dir` into memory on construction and writes updates on every mutation.
    pub fn with_sessions_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        // Load existing sessions from disk into the in-memory map.
        if let Ok(entries) = std::fs::read_dir(&dir) {
            if let Ok(mut map) = self.sessions.lock() {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("json") {
                        if let Ok(contents) = std::fs::read_to_string(&path) {
                            if let Ok(state) = serde_json::from_str::<SessionState>(&contents) {
                                map.insert(state.session_id.clone(), state);
                            }
                        }
                    }
                }
            }
        }
        self.sessions_dir = Some(dir);
        self
    }

    /// Write session state to `{sessions_dir}/{session_id}.json` if persistence is enabled.
    fn persist_session(&self, session_id: &str, state: &SessionState) {
        if let Some(ref dir) = self.sessions_dir {
            let path = dir.join(format!("{session_id}.json"));
            if let Ok(json) = serde_json::to_string_pretty(state) {
                let _ = std::fs::create_dir_all(dir);
                let _ = std::fs::write(&path, json);
            }
        }
    }

    /// Verify a session credential and dispatch by action.
    pub async fn verify_session_payload(
        &self,
        payload_json: &serde_json::Value,
        charge_amount_zat: u64,
    ) -> Result<SessionVerifyResult, SessionError> {
        let payload: SessionPayload = serde_json::from_value(payload_json.clone())
            .map_err(|e| SessionError::InvalidPayload(e.to_string()))?;

        match payload {
            SessionPayload::Open { deposit_txid, refund_address, bearer_secret } => {
                self.handle_open(&deposit_txid, &refund_address, bearer_secret.as_deref(), charge_amount_zat).await
            }
            SessionPayload::Bearer { session_id, bearer } => {
                self.handle_bearer(&session_id, &bearer, charge_amount_zat)
            }
            SessionPayload::TopUp { session_id, top_up_txid } => {
                self.handle_top_up(&session_id, &top_up_txid).await
            }
            SessionPayload::Close { session_id, bearer } => {
                self.handle_close(&session_id, &bearer).await
            }
        }
    }

    async fn handle_open(
        &self,
        deposit_txid: &str,
        refund_address: &str,
        bearer_secret: Option<&str>,
        charge_amount_zat: u64,
    ) -> Result<SessionVerifyResult, SessionError> {
        eprintln!("[session:open] Verifying deposit txid={}...", &deposit_txid[..16.min(deposit_txid.len())]);

        let result = shielded::verify_shielded(
            &self.rpc,
            &ShieldedVerifyRequest {
                txid: deposit_txid.to_string(),
                ivk_bytes_hex: self.orchard_ivk.clone(),
                expected_challenge_id: String::new(),
                expected_amount_zat: 0,
            },
            &self.consumed,
        )
        .await
        .map_err(|e| SessionError::VerificationFailed(e.to_string()))?;

        if result.outputs_decrypted == 0 {
            return Err(SessionError::VerificationFailed("no outputs decryptable".into()));
        }

        let deposit_amount = result.observed_amount_zat;
        let session_id = format!(
            "zs-{}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            &deposit_txid[..8.min(deposit_txid.len())]
        );

        let bearer_hash = if let Some(secret) = bearer_secret {
            sha256_hex(secret)
        } else {
            eprintln!("[session:open] WARNING: no bearerSecret provided, falling back to txid-based bearer (insecure)");
            sha256_hex(deposit_txid)
        };

        let state = SessionState {
            session_id: session_id.clone(),
            bearer_hash,
            deposit_amount_zat: deposit_amount,
            spent_zat: 0, // nothing charged on open — billing starts on first bearer/stream use
            refund_address: refund_address.to_string(),
            network: self.network.clone(),
            status: SessionStatus::Active,
        };

        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.insert(session_id.clone(), state.clone());
        }
        self.persist_session(&session_id, &state);

        eprintln!("[session:open] Created session {session_id}, deposit={deposit_amount}, charged={charge_amount_zat}");

        Ok(SessionVerifyResult { refund_txid: None, refund_amount_zat: None,
            session_id,
            action: "open".to_string(),
            is_management: true,
        })
    }

    fn handle_bearer(
        &self,
        session_id: &str,
        bearer: &str,
        charge_amount_zat: u64,
    ) -> Result<SessionVerifyResult, SessionError> {
        let mut sessions = self.sessions.lock()
            .map_err(|_| SessionError::LockError)?;

        let state = sessions.get_mut(session_id)
            .ok_or_else(|| SessionError::SessionNotFound(session_id.to_string()))?;

        if state.status != SessionStatus::Active {
            return Err(SessionError::SessionNotActive(session_id.to_string()));
        }

        if sha256_hex(bearer) != state.bearer_hash {
            return Err(SessionError::InvalidBearer);
        }

        let remaining = state.deposit_amount_zat.saturating_sub(state.spent_zat);
        if charge_amount_zat > remaining {
            return Err(SessionError::InsufficientBalance {
                needed: charge_amount_zat,
                available: remaining,
            });
        }

        state.spent_zat += charge_amount_zat;
        let new_remaining = state.deposit_amount_zat.saturating_sub(state.spent_zat);
        eprintln!("[session:bearer] {session_id}: charged {charge_amount_zat}, remaining {new_remaining}");
        let state_snapshot = state.clone();
        drop(sessions);
        self.persist_session(session_id, &state_snapshot);

        Ok(SessionVerifyResult { refund_txid: None, refund_amount_zat: None,
            session_id: session_id.to_string(),
            action: "bearer".to_string(),
            is_management: false,
        })
    }

    async fn handle_top_up(
        &self,
        session_id: &str,
        top_up_txid: &str,
    ) -> Result<SessionVerifyResult, SessionError> {
        eprintln!("[session:topUp] Verifying top-up txid={}...", &top_up_txid[..16.min(top_up_txid.len())]);

        let result = shielded::verify_shielded(
            &self.rpc,
            &ShieldedVerifyRequest {
                txid: top_up_txid.to_string(),
                ivk_bytes_hex: self.orchard_ivk.clone(),
                expected_challenge_id: String::new(),
                expected_amount_zat: 0,
            },
            &self.consumed,
        )
        .await
        .map_err(|e| SessionError::VerificationFailed(e.to_string()))?;

        if result.outputs_decrypted == 0 {
            return Err(SessionError::VerificationFailed("no outputs decryptable".into()));
        }

        let top_up_amount = result.observed_amount_zat;

        let mut sessions = self.sessions.lock()
            .map_err(|_| SessionError::LockError)?;
        let state = sessions.get_mut(session_id)
            .ok_or_else(|| SessionError::SessionNotFound(session_id.to_string()))?;

        state.deposit_amount_zat += top_up_amount;
        let new_remaining = state.deposit_amount_zat.saturating_sub(state.spent_zat);
        eprintln!("[session:topUp] {session_id}: added {top_up_amount}, new balance {new_remaining}");
        let state_snapshot = state.clone();
        drop(sessions);
        self.persist_session(session_id, &state_snapshot);

        Ok(SessionVerifyResult { refund_txid: None, refund_amount_zat: None,
            session_id: session_id.to_string(),
            action: "topUp".to_string(),
            is_management: true,
        })
    }

    async fn handle_close(
        &self,
        session_id: &str,
        bearer: &str,
    ) -> Result<SessionVerifyResult, SessionError> {
        // Collect everything we need from the lock in a non-async block,
        // so the MutexGuard is dropped before any .await point.
        let (refund_amount, cfg_clone, refund_addr, sid, closing_snapshot) = {
            let mut sessions = self.sessions.lock()
                .map_err(|_| SessionError::LockError)?;

            let state = sessions.get_mut(session_id)
                .ok_or_else(|| SessionError::SessionNotFound(session_id.to_string()))?;

            if state.status == SessionStatus::Closed || state.status == SessionStatus::Closing {
                return Err(SessionError::SessionNotActive(session_id.to_string()));
            }

            if sha256_hex(bearer) != state.bearer_hash {
                return Err(SessionError::InvalidBearer);
            }

            state.status = SessionStatus::Closing;

            let refund_amount = state.deposit_amount_zat.saturating_sub(state.spent_zat);
            eprintln!("[session:close] {session_id}: refund={refund_amount} to {}", &state.refund_address[..20.min(state.refund_address.len())]);

            let closing_snapshot = state.clone();
            (refund_amount, self.refund_config.clone(), state.refund_address.clone(), session_id.to_string(), closing_snapshot)
        }; // MutexGuard dropped here
        self.persist_session(&sid, &closing_snapshot);

        let actual_refund_txid = if refund_amount > 0 {
            if let Some(ref cfg) = cfg_clone {
                eprintln!("[session:close] Sending refund of {refund_amount} zat...");
                match send_refund(cfg, &refund_addr, refund_amount, &sid).await {
                    Ok(txid) => {
                        eprintln!("[session:close] Refund sent: {txid}");
                        Some(txid)
                    }
                    Err(e) => {
                        eprintln!("[session:close] Refund send failed: {e}");
                        None
                    }
                }
            } else {
                eprintln!("[session:close] No refund config — skipping refund of {refund_amount} zat");
                None
            }
        } else {
            None
        };

        // Re-acquire the lock to mark session as closed.
        let mut sessions = self.sessions.lock()
            .map_err(|_| SessionError::LockError)?;
        let state = sessions.get_mut(&sid)
            .ok_or_else(|| SessionError::SessionNotFound(sid.clone()))?;

        state.status = SessionStatus::Closed;
        eprintln!("[session:close] {session_id} closed. Total spent: {}", state.spent_zat);
        let closed_snapshot = state.clone();
        drop(sessions);
        self.persist_session(&sid, &closed_snapshot);

        Ok(SessionVerifyResult {
            refund_txid: actual_refund_txid,
            refund_amount_zat: Some(refund_amount),
            session_id: session_id.to_string(),
            action: "close".to_string(),
            is_management: true,
        })
    }

    /// Get session state (for debugging/monitoring).
    pub fn get_session(&self, session_id: &str) -> Option<SessionState> {
        self.sessions.lock().ok()?.get(session_id).cloned()
    }

    /// Deduct amount from session balance (for SSE streaming).
    /// Returns remaining balance after deduction.
    pub fn deduct(&self, session_id: &str, amount_zat: u64) -> Result<u64, SessionError> {
        let mut sessions = self.sessions.lock().map_err(|_| SessionError::LockError)?;
        let state = sessions.get_mut(session_id)
            .ok_or_else(|| SessionError::SessionNotFound(session_id.to_string()))?;
        if state.status != SessionStatus::Active {
            return Err(SessionError::SessionNotActive(session_id.to_string()));
        }
        let remaining = state.deposit_amount_zat.saturating_sub(state.spent_zat);
        if amount_zat > remaining {
            return Err(SessionError::InsufficientBalance { needed: amount_zat, available: remaining });
        }
        state.spent_zat += amount_zat;
        let remaining = state.deposit_amount_zat.saturating_sub(state.spent_zat);
        let state_snapshot = state.clone();
        drop(sessions);
        self.persist_session(session_id, &state_snapshot);
        Ok(remaining)
    }
}

// ── mpp-rs SessionMethod trait implementation ────────────────────────────

impl SessionMethod for ZcashSessionMethod {
    fn method(&self) -> &str {
        "zcash"
    }

    fn verify_session(
        &self,
        credential: &PaymentCredential,
        request: &SessionRequest,
    ) -> impl std::future::Future<Output = Result<Receipt, VerificationError>> + Send {
        let credential = credential.clone();
        let charge_amount: u64 = request.amount.parse().unwrap_or(0);
        let this = self.clone();

        async move {
            // credential.payload is already a serde_json::Value
            let payload_json = credential.payload;

            // Delegate to existing verify logic
            let result = this.verify_session_payload(&payload_json, charge_amount).await
                .map_err(|e| VerificationError::new(e.to_string()))?;

            // Cache the result for the respond() hook
            if let Ok(mut cache) = this.last_verify_results.lock() {
                cache.insert(result.session_id.clone(), result.clone());
            }

            Ok(Receipt::success("zcash", &result.session_id))
        }
    }

    fn challenge_method_details(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "network": self.network,
        }))
    }

    fn respond(&self, credential: &PaymentCredential, receipt: &Receipt) -> Option<serde_json::Value> {
        // credential.payload is already a serde_json::Value
        let action = credential.payload.get("action").and_then(|a| a.as_str()).unwrap_or("");

        match action {
            "open" | "topUp" | "close" => {
                // Look up cached verify result for management response details
                let session_id = &receipt.reference;
                let cached = self.last_verify_results.lock().ok()
                    .and_then(|cache| cache.get(session_id).cloned());

                let mut response = serde_json::json!({
                    "status": "ok",
                    "action": action,
                    "sessionId": session_id,
                });

                if let Some(result) = cached {
                    if let Some(ref txid) = result.refund_txid {
                        response["refundTxid"] = serde_json::json!(txid);
                    }
                    if let Some(amount) = result.refund_amount_zat {
                        response["refundAmountZat"] = serde_json::json!(amount);
                    }
                }

                // Clean up cached result
                if let Ok(mut cache) = self.last_verify_results.lock() {
                    cache.remove(session_id);
                }

                Some(response)
            }
            _ => None, // bearer — let the content handler run
        }
    }
}

/// Result of session verification.
#[derive(Debug, Clone, Serialize)]
pub struct SessionVerifyResult {
    pub session_id: String,
    pub action: String,
    pub is_management: bool,
    pub refund_txid: Option<String>,
    pub refund_amount_zat: Option<u64>,
}

#[derive(Debug, Clone)]
pub enum SessionError {
    InvalidPayload(String),
    VerificationFailed(String),
    SessionNotFound(String),
    SessionNotActive(String),
    InvalidBearer,
    InsufficientBalance { needed: u64, available: u64 },
    LockError,
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPayload(e) => write!(f, "invalid session payload: {e}"),
            Self::VerificationFailed(e) => write!(f, "verification failed: {e}"),
            Self::SessionNotFound(id) => write!(f, "session not found: {id}"),
            Self::SessionNotActive(id) => write!(f, "session not active: {id}"),
            Self::InvalidBearer => f.write_str("invalid bearer token"),
            Self::InsufficientBalance { needed, available } => {
                write!(f, "insufficient balance: need {needed}, have {available}")
            }
            Self::LockError => f.write_str("lock error"),
        }
    }
}

impl std::error::Error for SessionError {}

fn sha256_hex(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

/// Send a refund via the native wallet.
async fn send_refund(
    cfg: &zimppy_wallet::WalletConfig,
    to: &str,
    amount_zat: u64,
    session_id: &str,
) -> Result<String, zimppy_wallet::WalletError> {
    let mut wallet = zimppy_wallet::ZimppyWallet::open(zimppy_wallet::WalletConfig {
        data_dir: cfg.data_dir.clone(),
        lwd_endpoint: cfg.lwd_endpoint.clone(),
        network: cfg.network,
        seed_phrase: cfg.seed_phrase.clone(),
        birthday_height: cfg.birthday_height,
    }).await?;
    wallet.sync().await?;
    let memo = format!("zimppy-refund:{session_id}");
    wallet.send(to, amount_zat, Some(&memo)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_hex_works() {
        let hash = sha256_hex("test");
        assert_eq!(hash.len(), 64);
        assert_eq!(hash, "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08");
    }

    #[test]
    fn session_status_serializes() {
        let status = SessionStatus::Active;
        let json = serde_json::to_string(&status).expect("serialize");
        assert_eq!(json, "\"Active\"");
    }
}
