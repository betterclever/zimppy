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
/// id = HMAC-SHA256(secret, realm|method|intent|amount|currency|recipient|timestamp)
fn generate_challenge_id(secret: &str, fields: &[&str]) -> String {
    let payload = fields.join("|");
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(payload.as_bytes());
    let result = mac.finalize();
    hex::encode(result.into_bytes())
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
    let secret_key = std::env::var("MPP_SECRET_KEY")
        .unwrap_or_else(|_| "zimppy-demo-secret-key".to_string());

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
    eprintln!("  IVK: {}...{}", &config.orchard_ivk[..16], &config.orchard_ivk[config.orchard_ivk.len()-8..]);
    eprintln!("  price: {} zat per request", price);
    eprintln!("  RPC: {rpc_endpoint}");
    eprintln!("  port: {port}");
    eprintln!("  challenge IDs: HMAC-SHA256");

    let payment = ZcashChargeMethod::new(&rpc_endpoint, &config.address, &config.orchard_ivk);

    let wallet_dir = std::env::var("ZCASH_WALLET_DIR")
        .unwrap_or_else(|_| "/tmp/zcash-wallet-server".to_string());
    let identity_file = std::env::var("ZCASH_IDENTITY_FILE")
        .unwrap_or_else(|_| format!("{wallet_dir}/identity.txt"));
    let lwd_server = std::env::var("ZCASH_LWD_SERVER")
        .unwrap_or_else(|_| "testnet.zec.rocks:443".to_string());

    let session = ZcashSessionMethod::new(&rpc_endpoint, &config.orchard_ivk)
        .with_refund_config(RefundConfig {
            wallet_dir: wallet_dir.clone(),
            identity_file,
            lightwalletd_server: lwd_server,
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
        .route("/.well-known/payment", get(discovery))
        .with_state(state.clone());

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
        "intents": ["charge"],
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
    // No credential — issue session challenge (same as charge but intent=session)
    issue_challenge(state).await
}

async fn handle_session_payment(state: Arc<AppState>, auth_str: &str) -> axum::response::Response {
    let encoded = auth_str.trim_start_matches("Payment ").trim();
    eprintln!("[SESSION] Received credential");

    let bytes = match base64url_decode(encoded) {
        Ok(b) => b,
        Err(_) => return problem_response(400, "Invalid Credential", "invalid base64url"),
    };
    let value: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => return problem_response(400, "Invalid Credential", &format!("invalid JSON: {e}")),
    };

    let payload = match value.get("payload") {
        Some(p) => p.clone(),
        None => return problem_response(400, "Invalid Credential", "missing payload"),
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
                    [("payment-receipt", encoded_receipt)],
                    Json(serde_json::json!({
                        "status": "ok",
                        "sessionId": result.session_id,
                        "action": result.action,
                    })),
                )
                    .into_response()
            } else {
                // Bearer action → serve content
                let fortune = pick_fortune();
                eprintln!("[SESSION:200] Serving fortune via session: {fortune}");
                (
                    StatusCode::OK,
                    [("payment-receipt", encoded_receipt)],
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

// ── Items 1+2: HMAC challenge IDs + RFC 9457 problem details ────────────

async fn issue_challenge(state: Arc<AppState>) -> axum::response::Response {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Item 1: HMAC-SHA256 challenge ID per MPP spec
    let amount_str = state.amount_zat.to_string();
    let now_str = now.to_string();
    let challenge_id = generate_challenge_id(
        &state.secret_key,
        &["zimppy", "zcash", "charge", &amount_str, "ZEC", &state.config.address, &now_str],
    );

    let memo_template = format!("zimppy:{challenge_id}");

    eprintln!("[402] Issuing challenge:");
    eprintln!("  challenge_id: {challenge_id}");
    eprintln!("  recipient: {}", state.config.address);
    eprintln!("  amount: {} zat", state.amount_zat);
    eprintln!("  memo: {memo_template}");

    if let Ok(mut challenges) = state.challenges.lock() {
        challenges.insert(challenge_id.clone(), (state.amount_zat, now));
    }

    let request_payload = serde_json::json!({
        "challengeId": challenge_id,
        "amount": state.amount_zat.to_string(),
        "currency": "ZEC",
        "recipient": state.config.address,
        "network": state.config.network,
        "memo": memo_template,
        "expiresAt": now + 600,
    });

    let encoded_request = base64url_encode(&serde_json::to_vec(&request_payload).unwrap_or_default());

    let www_auth = format!(
        "Payment id=\"{challenge_id}\", realm=\"zimppy\", method=\"zcash\", intent=\"charge\", request=\"{encoded_request}\""
    );

    // Item 2: RFC 9457 problem details with application/problem+json
    (
        StatusCode::PAYMENT_REQUIRED,
        [
            (header::WWW_AUTHENTICATE, www_auth),
            (header::CONTENT_TYPE, PROBLEM_JSON.to_string()),
        ],
        Json(serde_json::json!({
            "type": "https://zimppy.dev/problems/payment-required",
            "title": "Payment Required",
            "status": 402,
            "detail": format!("Send {} zat to {} with memo '{}'", state.amount_zat, state.config.address, memo_template),
            "challengeId": challenge_id,
        })),
    )
        .into_response()
}

async fn handle_payment(state: Arc<AppState>, auth_str: &str) -> axum::response::Response {
    let encoded = auth_str.trim_start_matches("Payment ").trim();

    eprintln!("[AUTH] Received credential");

    let (txid, challenge_id) = match decode_credential(encoded) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[AUTH] ERROR: invalid credential: {e}");
            return problem_response(400, "Invalid Credential", &format!("invalid credential: {e}"));
        }
    };

    eprintln!("[AUTH] txid: {txid}");
    eprintln!("[AUTH] challenge_id: {challenge_id}");

    let amount_zat = match state.challenges.lock() {
        Ok(challenges) => {
            match challenges.get(&challenge_id) {
                Some(&(amount, _)) => amount,
                None => {
                    eprintln!("[AUTH] ERROR: unknown challenge_id");
                    return problem_response(400, "Unknown Challenge", "challenge_id not recognized");
                }
            }
        }
        Err(_) => {
            return problem_response(500, "Internal Error", "lock error");
        }
    };

    eprintln!("[VERIFY] Verifying shielded payment...");
    eprintln!("[VERIFY] amount: {amount_zat} zat, challenge: {challenge_id}");

    match state.payment.verify_payment(&txid, &challenge_id, amount_zat).await {
        Ok(outcome) => {
            eprintln!("[VERIFY] Result: verified={} amount={} memo_matched={} decrypted={}",
                outcome.verified, outcome.observed_amount_zat, outcome.memo_matched, outcome.outputs_decrypted);

            if outcome.verified {
                if let Ok(mut challenges) = state.challenges.lock() {
                    challenges.remove(&challenge_id);
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
                    [("payment-receipt", encoded_receipt)],
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
            "type": format!("https://zimppy.dev/problems/{}", title.to_lowercase().replace(' ', "-")),
            "title": title,
            "status": status,
            "detail": detail,
        })),
    )
        .into_response()
}

fn decode_credential(encoded: &str) -> Result<(String, String), String> {
    let bytes = base64url_decode(encoded).map_err(|_| "invalid base64url".to_string())?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("invalid JSON: {e}"))?;
    let txid = value
        .pointer("/payload/txid")
        .and_then(|v| v.as_str())
        .ok_or("missing payload.txid")?
        .to_string();
    let challenge_id = value
        .pointer("/payload/challengeId")
        .and_then(|v| v.as_str())
        .ok_or("missing payload.challengeId")?
        .to_string();
    Ok((txid, challenge_id))
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
