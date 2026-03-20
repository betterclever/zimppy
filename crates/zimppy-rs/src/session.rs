use std::collections::HashMap;
use std::process::Command;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use zimppy_core::replay::ConsumedTxids;
use zimppy_core::rpc::ZebradRpc;
use zimppy_core::shielded::{self, ShieldedVerifyRequest};

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

/// Config for server-side refund sending via zcash-devtool.
#[derive(Clone)]
pub struct RefundConfig {
    pub wallet_dir: String,
    pub identity_file: String,
    pub lightwalletd_server: String,
}

/// Zcash session method — manages prepaid balance sessions.
#[derive(Clone)]
pub struct ZcashSessionMethod {
    rpc: ZebradRpc,
    orchard_ivk: String,
    consumed: ConsumedTxids,
    sessions: Arc<Mutex<HashMap<String, SessionState>>>,
    refund_config: Option<RefundConfig>,
}

impl ZcashSessionMethod {
    pub fn new(rpc_endpoint: &str, orchard_ivk: &str) -> Self {
        Self {
            rpc: ZebradRpc::new(rpc_endpoint),
            orchard_ivk: orchard_ivk.to_string(),
            consumed: ConsumedTxids::new(),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            refund_config: None,
        }
    }

    pub fn with_refund_config(mut self, config: RefundConfig) -> Self {
        self.refund_config = Some(config);
        self
    }

    pub fn method(&self) -> &str {
        "zcash"
    }

    /// Verify a session credential and dispatch by action.
    pub async fn verify_session(
        &self,
        payload_json: &serde_json::Value,
        charge_amount_zat: u64,
    ) -> Result<SessionVerifyResult, SessionError> {
        let payload: SessionPayload = serde_json::from_value(payload_json.clone())
            .map_err(|e| SessionError::InvalidPayload(e.to_string()))?;

        match payload {
            SessionPayload::Open { deposit_txid, refund_address } => {
                self.handle_open(&deposit_txid, &refund_address, charge_amount_zat).await
            }
            SessionPayload::Bearer { session_id, bearer } => {
                self.handle_bearer(&session_id, &bearer, charge_amount_zat)
            }
            SessionPayload::TopUp { session_id, top_up_txid } => {
                self.handle_top_up(&session_id, &top_up_txid).await
            }
            SessionPayload::Close { session_id, bearer } => {
                self.handle_close(&session_id, &bearer)
            }
        }
    }

    async fn handle_open(
        &self,
        deposit_txid: &str,
        refund_address: &str,
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

        let bearer_hash = sha256_hex(deposit_txid);

        let state = SessionState {
            session_id: session_id.clone(),
            bearer_hash,
            deposit_amount_zat: deposit_amount,
            spent_zat: 0, // nothing charged on open — billing starts on first bearer/stream use
            refund_address: refund_address.to_string(),
            network: "testnet".to_string(),
            status: SessionStatus::Active,
        };

        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.insert(session_id.clone(), state);
        }

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

        Ok(SessionVerifyResult { refund_txid: None, refund_amount_zat: None,
            session_id: session_id.to_string(),
            action: "topUp".to_string(),
            is_management: true,
        })
    }

    fn handle_close(
        &self,
        session_id: &str,
        bearer: &str,
    ) -> Result<SessionVerifyResult, SessionError> {
        let mut sessions = self.sessions.lock()
            .map_err(|_| SessionError::LockError)?;

        let state = sessions.get_mut(session_id)
            .ok_or_else(|| SessionError::SessionNotFound(session_id.to_string()))?;

        if state.status == SessionStatus::Closed {
            return Err(SessionError::SessionNotActive(session_id.to_string()));
        }

        if sha256_hex(bearer) != state.bearer_hash {
            return Err(SessionError::InvalidBearer);
        }

        state.status = SessionStatus::Closing;

        let refund_amount = state.deposit_amount_zat.saturating_sub(state.spent_zat);
        eprintln!("[session:close] {session_id}: refund={refund_amount} to {}", &state.refund_address[..20.min(state.refund_address.len())]);

        let mut actual_refund_txid: Option<String> = None;

        if refund_amount > 0 {
            if let Some(ref cfg) = self.refund_config {
                eprintln!("[session:close] Sending refund of {refund_amount} zat...");

                let _sync = Command::new("zcash-devtool")
                    .args(["wallet", "-w", &cfg.wallet_dir, "sync",
                           "--server", &cfg.lightwalletd_server,
                           "--connection", "direct"])
                    .output();

                let send = Command::new("zcash-devtool")
                    .args(["wallet", "-w", &cfg.wallet_dir, "send",
                           "-i", &cfg.identity_file,
                           "--server", &cfg.lightwalletd_server,
                           "--connection", "direct",
                           "--address", &state.refund_address,
                           "--value", &refund_amount.to_string(),
                           "--memo", &format!("zimppy-refund:{session_id}")])
                    .output();

                match send {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        eprintln!("[session:close] send stdout: {stdout}");
                        eprintln!("[session:close] send stderr: {stderr}");
                        // zcash-devtool may output txid on stdout or stderr
                        let all_output = format!("{stdout}\n{stderr}");
                        let txid = all_output.lines()
                            .find(|l| l.trim().len() == 64 && l.trim().chars().all(|c| c.is_ascii_hexdigit()))
                            .map(|l| l.trim())
                            .unwrap_or("unknown");
                        eprintln!("[session:close] Refund sent: {txid}");
                        actual_refund_txid = Some(txid.to_string());
                    }
                    Err(e) => {
                        eprintln!("[session:close] Refund send failed: {e}");
                    }
                }
            } else {
                eprintln!("[session:close] No refund config — skipping refund of {refund_amount} zat");
            }
        }

        state.status = SessionStatus::Closed;
        eprintln!("[session:close] {session_id} closed. Total spent: {}", state.spent_zat);

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
        Ok(state.deposit_amount_zat.saturating_sub(state.spent_zat))
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
