use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use zimppy_rs::ZcashChargeMethod;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServerWalletConfig {
    network: String,
    address: String,
    orchard_ivk: String,
}

struct AppState {
    payment: ZcashChargeMethod,
    config: ServerWalletConfig,
    amount_zat: u64,
    /// Maps challenge_id -> (amount_zat, timestamp)
    challenges: Mutex<HashMap<String, (u64, u64)>>,
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

    let payment = ZcashChargeMethod::new(&rpc_endpoint, &config.address, &config.orchard_ivk);

    let state = Arc::new(AppState {
        payment,
        config,
        amount_zat: price,
        challenges: Mutex::new(HashMap::new()),
    });

    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/fortune", get(fortune))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap_or_else(|e| panic!("failed to bind: {e}"));

    eprintln!("  listening on http://0.0.0.0:{port}");
    eprintln!();

    axum::serve(listener, app)
        .await
        .unwrap_or_else(|e| panic!("server error: {e}"));
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok", "service": "zimppy-mpp-server" }))
}

async fn fortune(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Check for payment credential
    if let Some(auth) = headers.get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth.to_str() {
            if auth_str.starts_with("Payment ") {
                return handle_payment(state, auth_str).await;
            }
        }
    }

    // No credential — issue 402 challenge
    issue_challenge(state).await
}

async fn issue_challenge(state: Arc<AppState>) -> axum::response::Response {
    let challenge_id = uuid::Uuid::new_v4().to_string();
    let memo_template = format!("zimppy:{challenge_id}");

    eprintln!("[402] Issuing challenge:");
    eprintln!("  challenge_id: {challenge_id}");
    eprintln!("  recipient: {}", state.config.address);
    eprintln!("  amount: {} zat", state.amount_zat);
    eprintln!("  memo: {memo_template}");

    // Store challenge for later verification
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

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

    (
        StatusCode::PAYMENT_REQUIRED,
        [(header::WWW_AUTHENTICATE, www_auth)],
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

    // Decode credential
    let (txid, challenge_id) = match decode_credential(encoded) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[AUTH] ERROR: invalid credential: {e}");
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("invalid credential: {e}") })),
            )
                .into_response();
        }
    };

    eprintln!("[AUTH] txid: {txid}");
    eprintln!("[AUTH] challenge_id: {challenge_id}");

    // Look up the challenge
    let amount_zat = match state.challenges.lock() {
        Ok(challenges) => {
            match challenges.get(&challenge_id) {
                Some(&(amount, _)) => amount,
                None => {
                    eprintln!("[AUTH] ERROR: unknown challenge_id");
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({ "error": "unknown challenge_id" })),
                    )
                        .into_response();
                }
            }
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "lock error" })),
            )
                .into_response();
        }
    };

    eprintln!("[VERIFY] Verifying shielded payment...");
    eprintln!("[VERIFY] amount: {amount_zat} zat, challenge: {challenge_id}");

    // Verify the shielded payment
    match state.payment.verify_payment(&txid, &challenge_id, amount_zat).await {
        Ok(outcome) => {
            eprintln!("[VERIFY] Result: verified={} amount={} memo_matched={} decrypted={}",
                outcome.verified, outcome.observed_amount_zat, outcome.memo_matched, outcome.outputs_decrypted);

            if outcome.verified {
                // Remove used challenge
                if let Ok(mut challenges) = state.challenges.lock() {
                    challenges.remove(&challenge_id);
                }

                let receipt = serde_json::json!({
                    "status": "success",
                    "method": "zcash",
                    "reference": outcome.txid,
                    "amount": outcome.observed_amount_zat,
                    "challengeId": challenge_id,
                    "timestamp": std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                });

                let fortune = pick_fortune();
                eprintln!("[200] Payment verified! Serving fortune.");
                eprintln!("[200] Fortune: {fortune}");

                (
                    StatusCode::OK,
                    [("payment-receipt", serde_json::to_string(&receipt).unwrap_or_default())],
                    Json(serde_json::json!({ "fortune": fortune })),
                )
                    .into_response()
            } else {
                eprintln!("[402] Payment not verified: amount or memo mismatch");
                (
                    StatusCode::PAYMENT_REQUIRED,
                    Json(serde_json::json!({
                        "error": "payment not verified",
                        "verified": false,
                        "memo_matched": outcome.memo_matched,
                        "observed_amount": outcome.observed_amount_zat,
                    })),
                )
                    .into_response()
            }
        }
        Err(e) => {
            eprintln!("[VERIFY] ERROR: {e}");
            (
                StatusCode::PAYMENT_REQUIRED,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
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
