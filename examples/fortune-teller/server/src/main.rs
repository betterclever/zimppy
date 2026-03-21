use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;

use zimppy_rs::{ZcashChargeMethod, ZcashSessionMethod};
use zimppy_rs::session::RefundConfig;

type HmacSha256 = Hmac<Sha256>;

const PROBLEM_JSON: &str = "application/problem+json";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServerWalletConfig {
    network: String,
    address: String,
    orchard_ivk: String,
}

struct AppState {
    payment: ZcashChargeMethod,
    session: ZcashSessionMethod,
    config: ServerWalletConfig,
    amount_zat: u64,
    secret_key: String,
    /// Maps challenge_id -> (amount_zat, timestamp)
    challenges: Mutex<HashMap<String, (u64, u64)>>,
}

/// Generate HMAC-SHA256 challenge ID per MPP spec.
/// id = HMAC-SHA256(secret, realm|method|intent|request_b64|expires|nonce|scope)
fn generate_challenge_id(secret: &str, realm: &str, method: &str, intent: &str, request_b64: &str, expires: &str) -> String {
    let payload = format!("{realm}|{method}|{intent}|{request_b64}|{expires}||");
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(payload.as_bytes());
    let result = mac.finalize();
    base64url_encode(&result.into_bytes())
}

#[tokio::main]
async fn main() {
    let rpc_endpoint = std::env::var("ZEBRAD_RPC_ENDPOINT")
        .unwrap_or_else(|_| "https://zcash-testnet-zebrad.gateway.tatum.io".to_string());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3180);
    let price: u64 = std::env::var("PRICE_ZAT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(42_000);
    let secret_key = std::env::var("MPP_SECRET_KEY").unwrap_or_else(|_| {
        eprintln!("  WARNING: MPP_SECRET_KEY not set, using default (insecure for production)");
        "zimppy-demo-secret-key".to_string()
    });

    // Load server wallet config
    let config_path = std::env::var("SERVER_WALLET_CONFIG")
        .unwrap_or_else(|_| "config/server-wallet.json".to_string());
    let config_str = fs::read_to_string(&config_path)
        .unwrap_or_else(|e| panic!("failed to read {config_path}: {e}"));
    let config: ServerWalletConfig = serde_json::from_str(&config_str)
        .unwrap_or_else(|e| panic!("failed to parse {config_path}: {e}"));

    eprintln!("=== zimppy MPP server ===");
    eprintln!("  network: {}", config.network);
    eprintln!("  address: {}...{}", &config.address[..20], &config.address[config.address.len()-8..]);
    eprintln!("  IVK: (loaded, {} bytes)", config.orchard_ivk.len());
    eprintln!("  price: {} zat per request", price);
    eprintln!("  RPC: {rpc_endpoint}");
    eprintln!("  port: {port}");
    eprintln!("  challenge IDs: HMAC-SHA256");

    let payment = ZcashChargeMethod::new(&rpc_endpoint, &config.address, &config.orchard_ivk);

    let wallet_dir = std::env::var("ZIMPPY_WALLET_DIR")
        .unwrap_or_else(|_| "/tmp/zimppy-server-wallet".to_string());
    let lwd_endpoint = std::env::var("ZIMPPY_LWD_ENDPOINT")
        .unwrap_or_else(|_| "https://testnet.zec.rocks".to_string());
    let seed_phrase = std::env::var("ZIMPPY_SEED_PHRASE").ok();

    let session = ZcashSessionMethod::new(&rpc_endpoint, &config.orchard_ivk)
        .with_refund_config(RefundConfig {
            data_dir: std::path::PathBuf::from(&wallet_dir),
            lwd_endpoint,
            network: zcash_protocol::consensus::NetworkType::Test,
            seed_phrase,
            birthday_height: None,
        });

    let state = Arc::new(AppState {
        payment,
        session,
        config,
        amount_zat: price,
        secret_key,
        challenges: Mutex::new(HashMap::new()),
    });

    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/fortune", get(fortune))
        .route("/api/session/fortune", get(session_fortune))
        .route("/api/stream/fortune", get(stream_fortune))
        // Non-standard convenience endpoint (MPP discovery spec uses /openapi.json)
        .route("/.well-known/payment", get(discovery))
        .with_state(state.clone());

    eprintln!("  stream endpoint: /api/stream/fortune (SSE, 1000 zat/token)");

    eprintln!("  session endpoint: /api/session/fortune");

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap_or_else(|e| panic!("failed to bind: {e}"));

    eprintln!("  listening on http://0.0.0.0:{port}");
    eprintln!("  discovery: http://0.0.0.0:{port}/.well-known/payment");
    eprintln!();

    axum::serve(listener, app)
        .await
        .unwrap_or_else(|e| panic!("server error: {e}"));
}

// ── Item 3: Discovery endpoint ──────────────────────────────────────────

async fn discovery(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::json!({
        "methods": ["zcash"],
        "intents": ["charge", "session"],
        "network": state.config.network,
        "recipient": state.config.address,
        "defaultAmount": state.amount_zat.to_string(),
        "currency": "ZEC",
        "memo_format": "zimppy:{challenge_id}",
    }))
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok", "service": "zimppy-mpp-server" }))
}

async fn fortune(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Some(auth) = headers.get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth.to_str() {
            if auth_str.starts_with("Payment ") {
                return handle_payment(state, auth_str).await;
            }
        }
    }
    issue_challenge(state).await
}

// ── Session endpoint ────────────────────────────────────────────────────

async fn session_fortune(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Some(auth) = headers.get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth.to_str() {
            if auth_str.starts_with("Payment ") {
                return handle_session_payment(state, auth_str).await;
            }
        }
    }
    // No credential — issue session challenge (intent=session)
    issue_session_challenge(state).await
}

async fn handle_session_payment(state: Arc<AppState>, auth_str: &str) -> axum::response::Response {
    let encoded = auth_str.trim_start_matches("Payment ").trim();
    eprintln!("[SESSION] Received credential");

    let bytes = match base64url_decode(encoded) {
        Ok(b) => b,
        Err(_) => return problem_response(402, "Invalid Credential", "invalid base64url"),
    };
    let value: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => return problem_response(402, "Invalid Credential", &format!("invalid JSON: {e}")),
    };

    let payload = match value.get("payload") {
        Some(p) => p.clone(),
        None => return problem_response(402, "Invalid Credential", "missing payload"),
    };

    let action = payload.get("action").and_then(|a| a.as_str()).unwrap_or("unknown");
    eprintln!("[SESSION] Action: {action}");

    match state.session.verify_session(&payload, state.amount_zat).await {
        Ok(result) => {
            eprintln!("[SESSION] Result: session_id={}, action={}, management={}",
                result.session_id, result.action, result.is_management);

            let receipt = serde_json::json!({
                "status": "success",
                "method": "zcash",
                "reference": result.session_id,
                "action": result.action,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            });
            let encoded_receipt = base64url_encode(&serde_json::to_vec(&receipt).unwrap_or_default());

            if result.is_management {
                // Management actions (open, topUp, close) → return JSON directly
                (
                    StatusCode::OK,
                    [
                        ("payment-receipt", encoded_receipt),
                        ("cache-control", "private".to_string()),
                    ],
                    Json(serde_json::json!({
                        "status": "ok",
                        "sessionId": result.session_id,
                        "action": result.action,
                        "refundTxid": result.refund_txid,
                        "refundAmountZat": result.refund_amount_zat,
                    })),
                )
                    .into_response()
            } else {
                // Bearer action → serve content
                let fortune = pick_fortune();
                eprintln!("[SESSION:200] Serving fortune via session: {fortune}");
                (
                    StatusCode::OK,
                    [
                        ("payment-receipt", encoded_receipt),
                        ("cache-control", "private".to_string()),
                    ],
                    Json(serde_json::json!({ "fortune": fortune })),
                )
                    .into_response()
            }
        }
        Err(e) => {
            eprintln!("[SESSION] ERROR: {e}");
            problem_response(402, "Session Error", &e.to_string())
        }
    }
}

// ── SSE Streaming endpoint ──────────────────────────────────────────────

async fn stream_fortune(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Must have a session credential (bearer action)
    let auth = match headers.get(header::AUTHORIZATION).and_then(|h| h.to_str().ok()) {
        Some(a) if a.starts_with("Payment ") => a,
        _ => return issue_session_challenge(state).await,
    };

    let encoded = auth.trim_start_matches("Payment ").trim();
    let bytes = match base64url_decode(encoded) {
        Ok(b) => b,
        Err(_) => return problem_response(402, "Invalid Credential", "invalid base64url"),
    };
    let value: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => return problem_response(402, "Invalid Credential", &format!("invalid JSON: {e}")),
    };
    let payload = match value.get("payload") {
        Some(p) => p.clone(),
        None => return problem_response(402, "Invalid Credential", "missing payload"),
    };

    let action = payload.get("action").and_then(|a| a.as_str()).unwrap_or("");
    if action != "bearer" {
        return problem_response(400, "Invalid Action", "stream requires bearer action");
    }

    let session_id = payload.get("sessionId").and_then(|s| s.as_str()).unwrap_or("").to_string();
    let bearer = payload.get("bearer").and_then(|b| b.as_str()).unwrap_or("");

    // Verify bearer is valid
    let bearer_hash = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(bearer.as_bytes());
        hex::encode(hasher.finalize())
    };

    let session_state = match state.session.get_session(&session_id) {
        Some(s) => s,
        None => return problem_response(402, "Session Not Found", "session not found"),
    };

    if session_state.bearer_hash != bearer_hash {
        return problem_response(402, "Invalid Bearer", "invalid bearer token");
    }

    eprintln!("[STREAM] Starting SSE stream for session {session_id}");

    // Generate fortune tokens word by word
    let fortunes = [
        "Privacy is not about having something to hide. It is about having the power to choose what to share.",
        "In a world of surveillance, the shielded transaction is an act of freedom.",
        "Zero knowledge proofs: where math protects what matters most.",
        "The best encryption is the one that makes the data invisible, not just unreadable.",
    ];
    let fortune = fortunes[std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as usize % fortunes.len()];

    let words: Vec<String> = fortune.split_whitespace().map(String::from).collect();
    let tick_cost: u64 = 1000; // 1000 zat per word

    let stream = async_stream::stream! {
        let mut total_spent: u64 = 0;
        let mut total_chunks: u64 = 0;

        for word in &words {
            // Small delay for realistic streaming feel
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            match state.session.deduct(&session_id, tick_cost) {
                Ok(remaining) => {
                    total_spent += tick_cost;
                    total_chunks += 1;
                    let data = serde_json::json!({ "token": word, "remaining": remaining });
                    eprintln!("[STREAM] token=\"{word}\" remaining={remaining}");
                    yield Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default()
                        .event("message")
                        .data(data.to_string()));
                }
                Err(_) => {
                    let balance = state.session.get_session(&session_id)
                        .map(|s| s.deposit_amount_zat.saturating_sub(s.spent_zat))
                        .unwrap_or(0);
                    let need = serde_json::json!({
                        "sessionId": session_id,
                        "balanceRequired": tick_cost,
                        "balanceSpent": balance,
                    });
                    eprintln!("[STREAM] Balance exhausted after {total_chunks} tokens");
                    yield Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default()
                        .event("payment-need-topup")
                        .data(need.to_string()));
                    break;
                }
            }
        }

        // Receipt
        let receipt = serde_json::json!({
            "sessionId": session_id,
            "totalSpent": total_spent,
            "totalChunks": total_chunks,
        });
        eprintln!("[STREAM] Complete: {total_chunks} tokens, {total_spent} zat");
        yield Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default()
            .event("payment-receipt")
            .data(receipt.to_string()));
    };

    axum::response::sse::Sse::new(stream).into_response()
}

// ── Items 1+2: HMAC challenge IDs + RFC 9457 problem details ────────────

async fn issue_challenge(state: Arc<AppState>) -> axum::response::Response {
    let request_payload = serde_json::json!({
        "amount": state.amount_zat.to_string(),
        "currency": "zec",
        "recipient": state.config.address,
        "methodDetails": {
            "network": state.config.network,
            "memo": "zimppy:{id}",
        },
    });

    let encoded_request = base64url_encode(&serde_json::to_vec(&request_payload).unwrap_or_default());

    let expires_rfc3339 = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::seconds(600))
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339();

    let challenge_id = generate_challenge_id(
        &state.secret_key, "zimppy", "zcash", "charge", &encoded_request, &expires_rfc3339,
    );

    let memo_display = format!("zimppy:{challenge_id}");

    eprintln!("[402] Issuing challenge:");
    eprintln!("  challenge_id: {challenge_id}");
    eprintln!("  recipient: {}", state.config.address);
    eprintln!("  amount: {} zat", state.amount_zat);
    eprintln!("  memo: {memo_display}");

    if let Ok(mut challenges) = state.challenges.lock() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        challenges.insert(challenge_id.clone(), (state.amount_zat, now));
    }

    let www_auth = format!(
        "Payment id=\"{challenge_id}\", realm=\"zimppy\", method=\"zcash\", intent=\"charge\", request=\"{encoded_request}\", expires=\"{expires_rfc3339}\""
    );

    // Item 2: RFC 9457 problem details with application/problem+json
    (
        StatusCode::PAYMENT_REQUIRED,
        [
            (header::WWW_AUTHENTICATE, www_auth),
            (header::CONTENT_TYPE, PROBLEM_JSON.to_string()),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        Json(serde_json::json!({
            "type": "https://paymentauth.org/problems/payment-required",
            "title": "Payment Required",
            "status": 402,
            "detail": format!("Send {} zat to {} with memo '{}'", state.amount_zat, state.config.address, memo_display),
        })),
    )
        .into_response()
}

async fn issue_session_challenge(state: Arc<AppState>) -> axum::response::Response {
    let deposit_amount = state.amount_zat * 10;

    let request_payload = serde_json::json!({
        "amount": state.amount_zat.to_string(),
        "depositAmount": deposit_amount.to_string(),
        "currency": "zec",
        "recipient": state.config.address,
        "methodDetails": {
            "network": state.config.network,
            "memo": "zimppy:{id}",
        },
    });

    let encoded_request = base64url_encode(&serde_json::to_vec(&request_payload).unwrap_or_default());

    let expires_rfc3339 = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::seconds(600))
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339();

    let challenge_id = generate_challenge_id(
        &state.secret_key, "zimppy", "zcash", "session", &encoded_request, &expires_rfc3339,
    );

    let memo_display = format!("zimppy:{challenge_id}");

    eprintln!("[402] Issuing session challenge:");
    eprintln!("  challenge_id: {challenge_id}");
    eprintln!("  recipient: {}", state.config.address);
    eprintln!("  amount: {} zat", state.amount_zat);
    eprintln!("  memo: {memo_display}");

    if let Ok(mut challenges) = state.challenges.lock() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        challenges.insert(challenge_id.clone(), (state.amount_zat, now));
    }

    let www_auth = format!(
        "Payment id=\"{challenge_id}\", realm=\"zimppy\", method=\"zcash\", intent=\"session\", request=\"{encoded_request}\", expires=\"{expires_rfc3339}\""
    );

    (
        StatusCode::PAYMENT_REQUIRED,
        [
            (header::WWW_AUTHENTICATE, www_auth),
            (header::CONTENT_TYPE, PROBLEM_JSON.to_string()),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        Json(serde_json::json!({
            "type": "https://paymentauth.org/problems/payment-required",
            "title": "Payment Required",
            "status": 402,
            "detail": format!("Open a session by depositing {} zat to {} with memo '{}'", deposit_amount, state.config.address, memo_display),
        })),
    )
        .into_response()
}

async fn handle_payment(state: Arc<AppState>, auth_str: &str) -> axum::response::Response {
    let encoded = auth_str.trim_start_matches("Payment ").trim();

    eprintln!("[AUTH] Received credential");

    let cred = match decode_credential(encoded) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[AUTH] ERROR: invalid credential: {e}");
            return problem_response(402, "Invalid Credential", &format!("invalid credential: {e}"));
        }
    };

    let txid = &cred.txid;
    let challenge_id = &cred.challenge_id;

    eprintln!("[AUTH] txid: {txid}");
    eprintln!("[AUTH] challenge_id (from echoed challenge): {challenge_id}");

    // Stateless HMAC verification: recompute the challenge ID from echoed fields
    let recomputed_id = generate_challenge_id(
        &state.secret_key, &cred.realm, &cred.method, &cred.intent, &cred.request_b64, &cred.expires,
    );

    if recomputed_id != *challenge_id {
        eprintln!("[AUTH] ERROR: HMAC verification failed (recomputed={recomputed_id}, echoed={challenge_id})");
        return problem_response(402, "Invalid Challenge", "challenge HMAC verification failed");
    }

    eprintln!("[AUTH] HMAC verification passed (stateless)");

    // Secondary check: also verify against HashMap if available
    let amount_zat = match state.challenges.lock() {
        Ok(challenges) => {
            match challenges.get(challenge_id.as_str()) {
                Some(&(amount, _)) => amount,
                None => {
                    // HMAC passed but not in HashMap — use default amount
                    eprintln!("[AUTH] challenge not in HashMap, using default amount");
                    state.amount_zat
                }
            }
        }
        Err(_) => {
            return problem_response(500, "Internal Error", "lock error");
        }
    };

    eprintln!("[VERIFY] Verifying shielded payment...");
    eprintln!("[VERIFY] amount: {amount_zat} zat, challenge: {challenge_id}");

    match state.payment.verify_payment(txid, challenge_id, amount_zat).await {
        Ok(outcome) => {
            eprintln!("[VERIFY] Result: verified={} amount={} memo_matched={} decrypted={}",
                outcome.verified, outcome.observed_amount_zat, outcome.memo_matched, outcome.outputs_decrypted);

            if outcome.verified {
                if let Ok(mut challenges) = state.challenges.lock() {
                    challenges.remove(challenge_id.as_str());
                }

                let receipt = serde_json::json!({
                    "status": "success",
                    "method": "zcash",
                    "reference": outcome.txid,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                });
                let encoded_receipt =
                    base64url_encode(&serde_json::to_vec(&receipt).unwrap_or_default());

                let fortune = pick_fortune();
                eprintln!("[200] Payment verified! Serving fortune.");
                eprintln!("[200] Fortune: {fortune}");

                (
                    StatusCode::OK,
                    [
                        ("payment-receipt", encoded_receipt),
                        ("cache-control", "private".to_string()),
                    ],
                    Json(serde_json::json!({ "fortune": fortune })),
                )
                    .into_response()
            } else {
                eprintln!("[402] Payment not verified: amount or memo mismatch");
                problem_response(402, "Payment Not Verified", "amount or memo mismatch")
            }
        }
        Err(e) => {
            eprintln!("[VERIFY] ERROR: {e}");
            problem_response(402, "Verification Failed", &e.to_string())
        }
    }
}

/// RFC 9457 problem details response helper
fn problem_response(status: u16, title: &str, detail: &str) -> axum::response::Response {
    let code = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (
        code,
        [(header::CONTENT_TYPE, PROBLEM_JSON.to_string())],
        Json(serde_json::json!({
            "type": format!("https://paymentauth.org/problems/{}", title.to_lowercase().replace(' ', "-")),
            "title": title,
            "status": status,
            "detail": detail,
        })),
    )
        .into_response()
}

struct DecodedCredential {
    txid: String,
    challenge_id: String,
    realm: String,
    method: String,
    intent: String,
    request_b64: String,
    expires: String,
}

fn decode_credential(encoded: &str) -> Result<DecodedCredential, String> {
    let bytes = base64url_decode(encoded).map_err(|_| "invalid base64url".to_string())?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("invalid JSON: {e}"))?;
    let txid = value
        .pointer("/payload/txid")
        .and_then(|v| v.as_str())
        .ok_or("missing payload.txid")?
        .to_string();
    let challenge_id = value
        .pointer("/challenge/id")
        .and_then(|v| v.as_str())
        .ok_or("missing challenge.id")?
        .to_string();
    let realm = value
        .pointer("/challenge/realm")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let method = value
        .pointer("/challenge/method")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let intent = value
        .pointer("/challenge/intent")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let request_b64 = value
        .pointer("/challenge/request")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let expires = value
        .pointer("/challenge/expires")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok(DecodedCredential { txid, challenge_id, realm, method, intent, request_b64, expires })
}

fn pick_fortune() -> &'static str {
    let fortunes = [
        "Privacy is not about having something to hide.",
        "The best shield is the one nobody knows about.",
        "Zero knowledge, full power.",
        "A shielded transaction brings peace of mind.",
        "Trust in math, not middlemen.",
    ];
    let idx = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as usize
        % fortunes.len();
    fortunes[idx]
}

fn base64url_encode(data: &[u8]) -> String {
    use std::io::Write;
    let mut buf = Vec::new();
    let table = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut i = 0;
    while i + 2 < data.len() {
        let (b0, b1, b2) = (data[i], data[i + 1], data[i + 2]);
        let _ = buf.write_all(&[
            table[(b0 >> 2) as usize],
            table[((b0 & 0x03) << 4 | b1 >> 4) as usize],
            table[((b1 & 0x0f) << 2 | b2 >> 6) as usize],
            table[(b2 & 0x3f) as usize],
        ]);
        i += 3;
    }
    let remaining = data.len() - i;
    if remaining == 2 {
        let (b0, b1) = (data[i], data[i + 1]);
        let _ = buf.write_all(&[
            table[(b0 >> 2) as usize],
            table[((b0 & 0x03) << 4 | b1 >> 4) as usize],
            table[((b1 & 0x0f) << 2) as usize],
        ]);
    } else if remaining == 1 {
        let b0 = data[i];
        let _ = buf.write_all(&[
            table[(b0 >> 2) as usize],
            table[((b0 & 0x03) << 4) as usize],
        ]);
    }
    String::from_utf8(buf).unwrap_or_default()
}

fn base64url_decode(input: &str) -> Result<Vec<u8>, ()> {
    let input = input.replace('-', "+").replace('_', "/");
    let padded = match input.len() % 4 {
        2 => format!("{input}=="),
        3 => format!("{input}="),
        _ => input,
    };
    let table = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = Vec::new();
    let chars: Vec<u8> = padded.bytes().collect();
    for chunk in chars.chunks(4) {
        if chunk.len() < 4 { break; }
        let mut buf = [0u8; 4];
        for (i, &b) in chunk.iter().enumerate() {
            if b == b'=' { buf[i] = 0; }
            else { buf[i] = table.iter().position(|&t| t == b).ok_or(())? as u8; }
        }
        output.push((buf[0] << 2) | (buf[1] >> 4));
        if chunk[2] != b'=' { output.push((buf[1] << 4) | (buf[2] >> 2)); }
        if chunk[3] != b'=' { output.push((buf[2] << 6) | buf[3]); }
    }
    Ok(output)
}
