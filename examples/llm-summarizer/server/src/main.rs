use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;

use zimppy_rs::session::RefundConfig;
use zimppy_rs::ZcashChargeMethod;
use zimppy_rs::ZcashSessionMethod;
use zimppy_wallet::ZimppyWallet;

type HmacSha256 = Hmac<Sha256>;

const PROBLEM_JSON: &str = "application/problem+json";
const AI_SYSTEM_PROMPT: &str = "You are a concise document summarizer. Summarize the given text in 2-4 sentences, capturing the key points. Be direct and informative.";
const NO_SUMMARY: &str = "(no summary generated)";

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
    stream_tick_cost_zat: u64,
    secret_key: String,
    vibeproxy_url: String,
    vibeproxy_model: String,
    http_client: reqwest::Client,
    challenges: Mutex<HashMap<String, (u64, u64)>>,
}

fn generate_challenge_id(secret: &str, fields: &[&str]) -> String {
    let payload = fields.join("|");
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(payload.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

#[tokio::main]
async fn main() {
    let rpc_endpoint = std::env::var("ZEBRAD_RPC_ENDPOINT")
        .unwrap_or_else(|_| "https://zcash-testnet-zebrad.gateway.tatum.io".to_string());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3181);
    let price: u64 = std::env::var("PRICE_ZAT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(10_000);
    let stream_tick_cost: u64 = std::env::var("STREAM_TICK_COST_ZAT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(1_000);
    let secret_key = std::env::var("MPP_SECRET_KEY").unwrap_or_else(|_| {
        eprintln!("  WARNING: MPP_SECRET_KEY not set, using default (insecure for production)");
        "zimppy-ai-secret".to_string()
    });
    let vibeproxy_url =
        std::env::var("VIBEPROXY_URL").unwrap_or_else(|_| "http://localhost:8317".to_string());
    let vibeproxy_model =
        std::env::var("VIBEPROXY_MODEL").unwrap_or_else(|_| "gpt-5.4-mini".to_string());

    let config_path = std::env::var("SERVER_WALLET_CONFIG")
        .unwrap_or_else(|_| "config/server-wallet.json".to_string());
    let config_str = fs::read_to_string(&config_path)
        .unwrap_or_else(|e| panic!("failed to read {config_path}: {e}"));
    let config: ServerWalletConfig = serde_json::from_str(&config_str)
        .unwrap_or_else(|e| panic!("failed to parse {config_path}: {e}"));

    eprintln!("=== zimppy AI server ===");
    eprintln!("  network: {}", config.network);
    eprintln!(
        "  address: {}...{}",
        &config.address[..20],
        &config.address[config.address.len() - 8..]
    );
    eprintln!("  price: {} zat per summarization", price);
    eprintln!("  AI backend: {vibeproxy_url} (model: {vibeproxy_model})");
    eprintln!("  port: {port}");

    let payment = ZcashChargeMethod::new(&rpc_endpoint, &config.address, &config.orchard_ivk);

    let wallet_dir = std::env::var("ZIMPPY_WALLET_DIR")
        .unwrap_or_else(|_| "/tmp/zimppy-server-wallet".to_string());
    let lwd_endpoint = std::env::var("ZIMPPY_LWD_ENDPOINT")
        .unwrap_or_else(|_| "https://testnet.zec.rocks".to_string());
    let seed_phrase = std::env::var("ZIMPPY_SEED_PHRASE").ok();

    let refund_config = RefundConfig {
        data_dir: std::path::PathBuf::from(&wallet_dir),
        lwd_endpoint,
        network: zcash_protocol::consensus::NetworkType::Test,
        seed_phrase,
        birthday_height: None,
    };

    let session = match ZimppyWallet::open(refund_config.clone()).await {
        Ok(refund_wallet) => ZcashSessionMethod::new(&rpc_endpoint, &config.orchard_ivk)
            .with_refund_config(refund_config)
            .with_refund_wallet(refund_wallet),
        Err(e) => {
            eprintln!("  WARNING: refund wallet unavailable at startup: {e}");
            ZcashSessionMethod::new(&rpc_endpoint, &config.orchard_ivk)
                .with_refund_config(refund_config)
        }
    };

    let state = Arc::new(AppState {
        payment,
        session,
        config,
        amount_zat: price,
        stream_tick_cost_zat: stream_tick_cost,
        secret_key,
        vibeproxy_url,
        vibeproxy_model,
        http_client: reqwest::Client::new(),
        challenges: Mutex::new(HashMap::new()),
    });

    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/summarize", post(summarize))
        .route("/api/session/summarize", post(session_summarize))
        .route("/api/stream/summarize", post(stream_summarize))
        .route("/.well-known/payment", get(discovery))
        .with_state(state.clone());

    eprintln!("  endpoints:");
    eprintln!("    POST /api/summarize           (charge, {} zat)", price);
    eprintln!(
        "    POST /api/session/summarize    (session, {} zat/req)",
        price
    );
    eprintln!(
        "    POST /api/stream/summarize     (stream, {} zat/token)",
        stream_tick_cost
    );
    eprintln!("  listening on http://0.0.0.0:{port}");
    eprintln!("  discovery: http://0.0.0.0:{port}/.well-known/payment");
    eprintln!();

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap_or_else(|e| panic!("failed to bind: {e}"));

    axum::serve(listener, app)
        .await
        .unwrap_or_else(|e| panic!("server error: {e}"));
}

// ── AI backend ────────────────────────────────────────────────────────

async fn call_ai(state: &AppState, text: &str) -> Result<String, String> {
    let url = format!("{}/v1/chat/completions", state.vibeproxy_url);

    let body = serde_json::json!({
        "model": state.vibeproxy_model,
        "messages": [
            { "role": "system", "content": AI_SYSTEM_PROMPT },
            { "role": "user", "content": format!("Summarize the following text:\n\n{text}") }
        ],
        "max_tokens": 256,
    });

    eprintln!("[AI] Calling VibeProxy ({})...", state.vibeproxy_model);

    let resp = state
        .http_client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer dummy-not-used")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("AI request failed: {e}"))?;

    let status = resp.status();
    let resp_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("AI response parse failed: {e}"))?;

    if !status.is_success() {
        return Err(format!("AI returned {status}: {resp_body}"));
    }

    let summary = resp_body
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .unwrap_or(NO_SUMMARY)
        .to_string();

    eprintln!("[AI] Summary: {}...", &summary[..summary.len().min(80)]);
    Ok(summary)
}

// ── Discovery + health ────────────────────────────────────────────────

async fn discovery(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::json!({
        "service": "zimppy-ai-summarizer",
        "description": "Private document summarization powered by AI. Pay with Zcash, get a summary.",
        "methods": ["zcash"],
        "intents": ["charge", "session", "stream"],
        "network": state.config.network,
        "recipient": state.config.address,
        "currency": "ZEC",
        "endpoints": [
            {
                "path": "/api/summarize",
                "method": "POST",
                "intent": "charge",
                "amount": state.amount_zat.to_string(),
                "description": "Summarize a document (one-time payment)",
                "body": {"text": "Your document text here..."},
            },
            {
                "path": "/api/session/summarize",
                "method": "POST",
                "intent": "session",
                "amount": state.amount_zat.to_string(),
                "description": "Summarize via prepaid session (deposit once, many requests)",
                "body": {"text": "Your document text here..."},
            },
            {
                "path": "/api/stream/summarize",
                "method": "POST",
                "intent": "stream",
                "amount": state.stream_tick_cost_zat.to_string(),
                "unit": "zat/token",
                "description": "Streaming summary, pay per token via SSE",
                "body": {"text": "Your document text here..."},
            },
            {
                "path": "/api/health",
                "method": "GET",
                "amount": "0",
                "description": "Health check (free)",
            },
        ],
    }))
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok", "service": "zimppy-ai-summarizer" }))
}

// ── Charge endpoint: POST /api/summarize ──────────────────────────────

#[derive(Deserialize)]
struct SummarizeRequest {
    text: String,
}

async fn summarize(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Option<Json<SummarizeRequest>>,
) -> impl IntoResponse {
    if let Some(auth) = headers.get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth.to_str() {
            if auth_str.starts_with("Payment ") {
                return handle_charge_payment(state, auth_str, body).await;
            }
        }
    }
    issue_challenge(state).await
}

async fn handle_charge_payment(
    state: Arc<AppState>,
    auth_str: &str,
    body: Option<Json<SummarizeRequest>>,
) -> axum::response::Response {
    let encoded = auth_str.trim_start_matches("Payment ").trim();
    eprintln!("[AUTH] Received credential");

    let (txid, challenge_id) = match decode_credential(encoded) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[AUTH] ERROR: {e}");
            return problem_response(
                400,
                "Invalid Credential",
                &format!("invalid credential: {e}"),
            );
        }
    };

    eprintln!("[AUTH] txid: {txid}");
    eprintln!("[AUTH] challenge_id: {challenge_id}");

    let amount_zat = match state.challenges.lock() {
        Ok(challenges) => match challenges.get(&challenge_id) {
            Some(&(amount, _)) => amount,
            None => {
                return problem_response(400, "Unknown Challenge", "challenge_id not recognized")
            }
        },
        Err(_) => return problem_response(500, "Internal Error", "lock error"),
    };

    eprintln!("[VERIFY] Verifying shielded payment...");
    match state
        .payment
        .verify_payment(&txid, &challenge_id, amount_zat)
        .await
    {
        Ok(outcome) => {
            eprintln!(
                "[VERIFY] verified={} amount={} memo_matched={}",
                outcome.verified, outcome.observed_amount_zat, outcome.memo_matched
            );

            if !outcome.verified {
                return problem_response(402, "Payment Not Verified", "amount or memo mismatch");
            }

            if let Ok(mut challenges) = state.challenges.lock() {
                challenges.remove(&challenge_id);
            }

            let text = match &body {
                Some(b) => &b.text,
                None => {
                    return problem_response(
                        400,
                        "Missing Body",
                        "POST body with {\"text\": \"...\"} required",
                    )
                }
            };

            let summary = match call_ai(&state, text).await {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[AI] ERROR: {e}");
                    return problem_response(500, "AI Error", &e);
                }
            };

            let receipt = serde_json::json!({
                "status": "success",
                "method": "zcash",
                "reference": outcome.txid,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            });
            let encoded_receipt =
                base64url_encode(&serde_json::to_vec(&receipt).unwrap_or_default());

            eprintln!("[200] Payment verified, summary served");

            (
                StatusCode::OK,
                [("payment-receipt", encoded_receipt)],
                Json(serde_json::json!({ "summary": summary })),
            )
                .into_response()
        }
        Err(e) => {
            eprintln!("[VERIFY] ERROR: {e}");
            problem_response(402, "Verification Failed", &e.to_string())
        }
    }
}

// ── Session endpoint: POST /api/session/summarize ─────────────────────

async fn session_summarize(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Option<Json<SummarizeRequest>>,
) -> impl IntoResponse {
    if let Some(auth) = headers.get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth.to_str() {
            if auth_str.starts_with("Payment ") {
                return handle_session_payment(state, auth_str, body).await;
            }
        }
    }
    issue_challenge(state).await
}

async fn handle_session_payment(
    state: Arc<AppState>,
    auth_str: &str,
    body: Option<Json<SummarizeRequest>>,
) -> axum::response::Response {
    let encoded = auth_str.trim_start_matches("Payment ").trim();
    eprintln!("[SESSION] Received credential");

    let bytes = match base64url_decode(encoded) {
        Ok(b) => b,
        Err(_) => return problem_response(400, "Invalid Credential", "invalid base64url"),
    };
    let value: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            return problem_response(400, "Invalid Credential", &format!("invalid JSON: {e}"))
        }
    };
    let payload = match value.get("payload") {
        Some(p) => p.clone(),
        None => return problem_response(400, "Invalid Credential", "missing payload"),
    };

    let action = payload
        .get("action")
        .and_then(|a| a.as_str())
        .unwrap_or("unknown");
    eprintln!("[SESSION] Action: {action}");

    match state
        .session
        .verify_session_payload(&payload, state.amount_zat, state.amount_zat * 10)
        .await
    {
        Ok(result) => {
            eprintln!(
                "[SESSION] session_id={}, action={}, management={}",
                result.session_id, result.action, result.is_management
            );

            let receipt = serde_json::json!({
                "status": "success",
                "method": "zcash",
                "reference": result.session_id,
                "action": result.action,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            });
            let encoded_receipt =
                base64url_encode(&serde_json::to_vec(&receipt).unwrap_or_default());

            if result.is_management {
                (
                    StatusCode::OK,
                    [("payment-receipt", encoded_receipt)],
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
                // Bearer — do the actual summarization
                let text = match &body {
                    Some(b) => &b.text,
                    None => {
                        return problem_response(
                            400,
                            "Missing Body",
                            "POST body with {\"text\": \"...\"} required",
                        )
                    }
                };

                let summary = match call_ai(&state, text).await {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("[AI] ERROR: {e}");
                        return problem_response(500, "AI Error", &e);
                    }
                };

                eprintln!("[SESSION:200] Summary served via session");

                (
                    StatusCode::OK,
                    [("payment-receipt", encoded_receipt)],
                    Json(serde_json::json!({ "summary": summary })),
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

// ── SSE Streaming endpoint: POST /api/stream/summarize ────────────────

async fn stream_summarize(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Option<Json<SummarizeRequest>>,
) -> impl IntoResponse {
    let auth = match headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
    {
        Some(a) if a.starts_with("Payment ") => a,
        _ => return issue_challenge(state).await,
    };

    let encoded = auth.trim_start_matches("Payment ").trim();
    let bytes = match base64url_decode(encoded) {
        Ok(b) => b,
        Err(_) => return problem_response(400, "Invalid Credential", "invalid base64url"),
    };
    let value: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            return problem_response(400, "Invalid Credential", &format!("invalid JSON: {e}"))
        }
    };
    let payload = match value.get("payload") {
        Some(p) => p.clone(),
        None => return problem_response(400, "Invalid Credential", "missing payload"),
    };

    let action = payload.get("action").and_then(|a| a.as_str()).unwrap_or("");
    if action != "bearer" {
        return problem_response(400, "Invalid Action", "stream requires bearer action");
    }

    let session_id = payload
        .get("sessionId")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();
    let bearer = payload.get("bearer").and_then(|b| b.as_str()).unwrap_or("");

    // Verify bearer
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

    let text = match &body {
        Some(b) => b.text.clone(),
        None => {
            return problem_response(
                400,
                "Missing Body",
                "POST body with {\"text\": \"...\"} required",
            )
        }
    };

    eprintln!("[STREAM] Starting AI stream for session {session_id}");

    let summary = match call_ai(&state, &text).await {
        Ok(s) => s,
        Err(e) => return problem_response(500, "AI Error", &e),
    };

    // Stream the summary word by word, deducting per token
    let words: Vec<String> = summary.split_whitespace().map(String::from).collect();
    let tick_cost: u64 = state.stream_tick_cost_zat;

    let stream = async_stream::stream! {
        let mut total_spent: u64 = 0;
        let mut total_chunks: u64 = 0;

        for word in &words {
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;

            match state.session.deduct(&session_id, tick_cost) {
                Ok(remaining) => {
                    total_spent += tick_cost;
                    total_chunks += 1;
                    let data = serde_json::json!({ "token": word, "remaining": remaining });
                    eprintln!("[STREAM] token=\"{word}\" cost={tick_cost} remaining={remaining}");
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
                        "requiredAmount": tick_cost,
                        "currentBalance": balance,
                    });
                    eprintln!("[STREAM] Balance exhausted after {total_chunks} tokens");
                    yield Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default()
                        .event("payment-need-topup")
                        .data(need.to_string()));
                    break;
                }
            }
        }

        let receipt = serde_json::json!({
            "sessionId": session_id,
            "totalSpent": total_spent,
            "totalTokens": total_chunks,
            "costPerToken": tick_cost,
        });
        eprintln!("[STREAM] Complete: {total_chunks} tokens, {total_spent} zat");
        yield Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default()
            .event("payment-receipt")
            .data(receipt.to_string()));
    };

    axum::response::sse::Sse::new(stream).into_response()
}

// ── Challenge + helpers ───────────────────────────────────────────────

async fn issue_challenge(state: Arc<AppState>) -> axum::response::Response {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let amount_str = state.amount_zat.to_string();
    let now_str = now.to_string();
    let challenge_id = generate_challenge_id(
        &state.secret_key,
        &[
            "zimppy-ai",
            "zcash",
            "charge",
            &amount_str,
            "ZEC",
            &state.config.address,
            &now_str,
        ],
    );

    let memo_template = format!("zimppy:{challenge_id}");

    eprintln!(
        "[402] Issuing challenge: {} zat, memo={}",
        state.amount_zat,
        &memo_template[..40.min(memo_template.len())]
    );

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
    let encoded_request =
        base64url_encode(&serde_json::to_vec(&request_payload).unwrap_or_default());

    let www_auth = format!(
        "Payment id=\"{challenge_id}\", realm=\"zimppy-ai\", method=\"zcash\", intent=\"charge\", request=\"{encoded_request}\""
    );

    (
        StatusCode::PAYMENT_REQUIRED,
        [
            (header::WWW_AUTHENTICATE, www_auth),
            (header::CONTENT_TYPE, PROBLEM_JSON.to_string()),
        ],
        Json(serde_json::json!({
            "type": "https://paymentauth.org/problems/payment-required",
            "title": "Payment Required",
            "status": 402,
            "detail": format!("Send {} zat to summarize your document", state.amount_zat),
            "challengeId": challenge_id,
        })),
    )
        .into_response()
}

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
    ).into_response()
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
    let padded = format!("{}{}", input, &"===="[..((4 - input.len() % 4) % 4)]);
    let standard = padded.replace('-', "+").replace('_', "/");
    let decoded = standard
        .as_bytes()
        .iter()
        .filter_map(|&b| match b {
            b'A'..=b'Z' => Some(b - b'A'),
            b'a'..=b'z' => Some(b - b'a' + 26),
            b'0'..=b'9' => Some(b - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            b'=' => None,
            _ => None,
        })
        .collect::<Vec<u8>>();

    let mut result = Vec::new();
    let mut i = 0;
    while i + 3 < decoded.len() {
        result.push((decoded[i] << 2) | (decoded[i + 1] >> 4));
        result.push((decoded[i + 1] << 4) | (decoded[i + 2] >> 2));
        result.push((decoded[i + 2] << 6) | decoded[i + 3]);
        i += 4;
    }
    let remaining = decoded.len() - i;
    if remaining >= 2 {
        result.push((decoded[i] << 2) | (decoded[i + 1] >> 4));
    }
    if remaining >= 3 {
        result.push((decoded[i + 1] << 4) | (decoded[i + 2] >> 2));
    }
    Ok(result)
}
