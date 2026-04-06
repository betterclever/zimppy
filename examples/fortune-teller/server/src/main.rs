use std::fs;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use mpp::protocol::core::{Base64UrlJson, PaymentChallenge};
use mpp::protocol::intents::SessionRequest;
use mpp::server::axum::{ChargeChallenger, ChargeConfig, MppCharge, WithReceipt};
use mpp::server::Mpp;
use mpp::{compute_challenge_id, parse_authorization};
use zimppy_rs::axum::ZcashChallenger;
use zimppy_rs::session::RefundConfig;
use zimppy_rs::{ZcashChargeMethod, ZcashSessionMethod};
use zimppy_wallet::ZimppyWallet;

const PROBLEM_JSON: &str = "application/problem+json";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServerWalletConfig {
    network: String,
    address: String,
    orchard_ivk: String,
}

// ── Charge pricing ──────────────────────────────────────────────

struct FortunePrice;

impl ChargeConfig for FortunePrice {
    fn amount() -> &'static str {
        "42000"
    }
    fn description() -> Option<&'static str> {
        Some("Fortune telling - one shielded payment, one fortune")
    }
}

// ── App state ───────────────────────────────────────────────────

struct AppState {
    challenger: Arc<dyn ChargeChallenger>,
    /// Keep the Mpp for session verification.
    mpp: Mpp<ZcashChargeMethod, ZcashSessionMethod>,
    /// Keep a clone for direct session access (deduct, get_session for SSE streaming).
    session: ZcashSessionMethod,
    config: ServerWalletConfig,
    amount_zat: u64,
    secret_key: String,
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
    eprintln!(
        "  address: {}...{}",
        &config.address[..20],
        &config.address[config.address.len() - 8..]
    );
    eprintln!("  IVK: (loaded, {} bytes)", config.orchard_ivk.len());
    eprintln!("  price: {} zat per request", price);
    eprintln!("  RPC: {rpc_endpoint}");
    eprintln!("  port: {port}");
    eprintln!("  challenge IDs: HMAC-SHA256 (via mpp-rs)");

    let charge = ZcashChargeMethod::new(&rpc_endpoint, &config.address, &config.orchard_ivk);

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
        account_index: 0,
        num_accounts: 1,
        passphrase: None,
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

    let mpp = Mpp::new(charge.clone(), "zimppy", &secret_key).with_session_method(session.clone());

    let challenger: Arc<dyn ChargeChallenger> = Arc::new(ZcashChallenger::from_mpp(
        Mpp::new(charge, "zimppy", &secret_key),
        &secret_key,
        "zimppy",
        &config.address,
        &config.network,
    ));

    let state = Arc::new(AppState {
        challenger,
        mpp,
        session,
        config,
        amount_zat: price,
        secret_key,
    });

    // Charge endpoint uses MppCharge extractor (needs Arc<dyn ChargeChallenger> state)
    let charge_router = Router::new()
        .route("/api/fortune", get(fortune))
        .with_state(state.challenger.clone());

    // Session/stream/discovery use AppState
    let app_router = Router::new()
        .route("/api/health", get(health))
        .route("/api/session/fortune", get(session_fortune))
        .route("/api/stream/fortune", get(stream_fortune))
        .route("/.well-known/payment", get(discovery))
        .with_state(state.clone());

    let app = charge_router.merge(app_router);

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

// ── Discovery endpoint ──────────────────────────────────────────

async fn discovery(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::json!({
        "methods": ["zcash"],
        "intents": ["charge", "session", "stream"],
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

// ── Charge endpoint (MppCharge extractor) ───────────────────────

async fn fortune(charge: MppCharge<FortunePrice>) -> WithReceipt<Json<serde_json::Value>> {
    let fortune = pick_fortune();
    eprintln!("[200] Payment verified! Serving fortune: {fortune}");
    WithReceipt {
        receipt: charge.receipt,
        body: Json(serde_json::json!({ "fortune": fortune })),
    }
}

// ── Session endpoint ────────────────────────────────────────────

async fn session_fortune(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Some(credential) = parse_credential(&headers) {
        match state.mpp.verify_session(&credential).await {
            Ok(result) => {
                let receipt_header = result.receipt.to_header().unwrap_or_default();

                // Management response (open, close, topUp)
                if let Some(mgmt) = result.management_response {
                    eprintln!("[SESSION:200] Management response");
                    return (
                        StatusCode::OK,
                        [
                            ("payment-receipt", receipt_header),
                            ("cache-control", "private".to_string()),
                        ],
                        Json(mgmt),
                    )
                        .into_response();
                }

                // Content response (bearer)
                let fortune = pick_fortune();
                eprintln!("[SESSION:200] Serving fortune via session: {fortune}");
                return (
                    StatusCode::OK,
                    [
                        ("payment-receipt", receipt_header),
                        ("cache-control", "private".to_string()),
                    ],
                    Json(serde_json::json!({ "fortune": fortune })),
                )
                    .into_response();
            }
            Err(e) => {
                eprintln!("[SESSION] Error: {e}");
                // Fall through to issue new challenge
            }
        }
    }
    issue_session_challenge(&state).into_response()
}

fn issue_session_challenge(state: &AppState) -> axum::response::Response {
    let deposit_amount = state.amount_zat * 10;

    let request = SessionRequest {
        amount: state.amount_zat.to_string(),
        currency: "zec".to_string(),
        recipient: Some(state.config.address.clone()),
        suggested_deposit: Some(deposit_amount.to_string()),
        method_details: Some(serde_json::json!({
            "memo": "zimppy:{id}",
            "network": state.config.network,
        })),
        ..Default::default()
    };

    match build_challenge(state, "session", &request) {
        Ok(challenge) => {
            let memo_display = format!("zimppy:{}", challenge.id);
            eprintln!("[402] Issuing session challenge:");
            eprintln!("  challenge_id: {}", challenge.id);
            eprintln!("  recipient: {}", state.config.address);
            eprintln!("  amount: {} zat", state.amount_zat);
            eprintln!("  memo: {memo_display}");

            let www_auth = challenge.to_header().unwrap_or_default();
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
            ).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ── SSE Streaming endpoint ──────────────────────────────────────

async fn stream_fortune(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Must have a session credential (bearer action)
    let credential = match parse_credential(&headers) {
        Some(c) => c,
        None => return issue_session_challenge(&state).into_response(),
    };

    // Verify via Mpp (HMAC + expiry + session method)
    let result = match state.mpp.verify_session(&credential).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[STREAM] Verify error: {e}");
            return issue_session_challenge(&state).into_response();
        }
    };

    // Management actions should not go to streaming
    if result.management_response.is_some() {
        let receipt_header = result.receipt.to_header().unwrap_or_default();
        return (
            StatusCode::OK,
            [
                ("payment-receipt", receipt_header),
                ("cache-control", "private".to_string()),
            ],
            Json(result.management_response.unwrap()),
        )
            .into_response();
    }

    let session_id = result.receipt.reference.clone();
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
        .subsec_nanos() as usize
        % fortunes.len()];

    let words: Vec<String> = fortune.split_whitespace().map(String::from).collect();
    let tick_cost: u64 = 1000; // 1000 zat per word
    let top_up_timeout = std::time::Duration::from_secs(300);
    let poll_interval = std::time::Duration::from_secs(1);

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

                    let deadline = std::time::Instant::now() + top_up_timeout;
                    let mut funded = false;

                    while std::time::Instant::now() < deadline {
                        tokio::time::sleep(poll_interval).await;
                        match state.session.deduct(&session_id, tick_cost) {
                            Ok(remaining) => {
                                total_spent += tick_cost;
                                total_chunks += 1;
                                let data = serde_json::json!({ "token": word, "remaining": remaining });
                                eprintln!("[STREAM] top-up received, resuming with token=\"{word}\" remaining={remaining}");
                                yield Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default()
                                    .event("message")
                                    .data(data.to_string()));
                                funded = true;
                                break;
                            }
                            Err(_) => (),
                        }
                    }

                    if !funded {
                        eprintln!("[STREAM] top-up timed out");
                        yield Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default()
                            .event("error")
                            .data(serde_json::json!({ "error": "topUp timeout" }).to_string()));
                        break;
                    }
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

// ── Challenge construction ──────────────────────────────────────

/// Build a PaymentChallenge for the Zcash method using mpp-rs HMAC.
fn build_challenge<T: serde::Serialize>(
    state: &AppState,
    intent: &str,
    request: &T,
) -> Result<PaymentChallenge, mpp::MppError> {
    let encoded = Base64UrlJson::from_typed(request)?;

    let expires = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::seconds(600))
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339();

    let id = compute_challenge_id(
        &state.secret_key,
        state.mpp.realm(),
        state.mpp.method_name(),
        intent,
        encoded.raw(),
        Some(&expires),
        None,
        None,
    );

    Ok(PaymentChallenge {
        id,
        realm: state.mpp.realm().to_string(),
        method: state.mpp.method_name().into(),
        intent: intent.into(),
        request: encoded,
        expires: Some(expires),
        description: None,
        digest: None,
        opaque: None,
    })
}

// ── Helpers ─────────────────────────────────────────────────────

fn parse_credential(headers: &HeaderMap) -> Option<mpp::PaymentCredential> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| parse_authorization(s).ok())
}

/// RFC 9457 problem details response helper
#[allow(dead_code)]
fn problem_response(
    status: u16,
    title: &str,
    detail: &str,
    problem_type: &str,
) -> axum::response::Response {
    let code = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (
        code,
        [(header::CONTENT_TYPE, PROBLEM_JSON.to_string())],
        Json(serde_json::json!({
            "type": format!("https://paymentauth.org/problems/{problem_type}"),
            "title": title,
            "status": status,
            "detail": detail,
        })),
    )
        .into_response()
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
