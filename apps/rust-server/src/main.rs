use std::sync::Arc;

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};

use zimppy_rs::ZcashChargeMethod;

struct AppState {
    payment: ZcashChargeMethod,
    amount_zat: u64,
}

#[tokio::main]
async fn main() {
    let rpc_endpoint = std::env::var("ZEBRAD_RPC_ENDPOINT")
        .unwrap_or_else(|_| "https://zcash-testnet-zebrad.gateway.tatum.io".to_string());
    let recipient = std::env::var("ZCASH_RECIPIENT")
        .unwrap_or_else(|_| "tmHQEhKoEkBFR49E6dGG1QCMz4VEBrTpjCp".to_string());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3180);

    let state = Arc::new(AppState {
        payment: ZcashChargeMethod::new(&rpc_endpoint, &recipient),
        amount_zat: std::env::var("PRICE_ZAT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(42_000),
    });

    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/fortune", get(fortune))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap_or_else(|e| panic!("failed to bind: {e}"));

    println!("zimppy rust server on http://0.0.0.0:{port}");
    println!("  recipient: {recipient}");
    println!("  rpc: {rpc_endpoint}");
    println!("  price: {} zat per request", state.amount_zat);

    axum::serve(listener, app)
        .await
        .unwrap_or_else(|e| panic!("server error: {e}"));
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok", "service": "zimppy-rust-server" }))
}

async fn fortune(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Check for payment credential
    if let Some(auth) = headers.get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth.to_str() {
            if auth_str.starts_with("Payment ") {
                let encoded = auth_str.trim_start_matches("Payment ").trim();
                match decode_credential(encoded) {
                    Ok((txid, output_index)) => {
                        match state
                            .payment
                            .verify_payment(&txid, output_index, state.amount_zat)
                            .await
                        {
                            Ok(outcome) if outcome.verified => {
                                let receipt = serde_json::json!({
                                    "status": "success",
                                    "method": "zcash",
                                    "reference": outcome.txid,
                                    "timestamp": chrono_now(),
                                });
                                let receipt_header = serde_json::to_string(&receipt)
                                    .unwrap_or_default();

                                let fortunes = [
                                    "Privacy is not about having something to hide.",
                                    "The best shield is the one nobody knows about.",
                                    "Zero knowledge, full power.",
                                    "A shielded transaction brings peace of mind.",
                                    "Trust in math, not middlemen.",
                                ];
                                let fortune = fortunes[rand_index(fortunes.len())];

                                return (
                                    StatusCode::OK,
                                    [("payment-receipt", receipt_header)],
                                    Json(serde_json::json!({ "fortune": fortune })),
                                )
                                    .into_response();
                            }
                            Ok(_) => {
                                return (
                                    StatusCode::PAYMENT_REQUIRED,
                                    Json(serde_json::json!({ "error": "payment not verified" })),
                                )
                                    .into_response();
                            }
                            Err(e) => {
                                return (
                                    StatusCode::PAYMENT_REQUIRED,
                                    Json(serde_json::json!({ "error": e.to_string() })),
                                )
                                    .into_response();
                            }
                        }
                    }
                    Err(e) => {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(serde_json::json!({ "error": e })),
                        )
                            .into_response();
                    }
                }
            }
        }
    }

    // No credential — return 402 with challenge
    let challenge_id = simple_uuid();
    let expires_at = chrono_future(600); // 10 minutes
    let request = serde_json::json!({
        "challengeId": challenge_id,
        "amount": state.amount_zat.to_string(),
        "currency": "ZEC",
        "recipient": state.payment.recipient(),
        "network": "testnet",
        "expiresAt": expires_at,
    });

    let encoded_request = base64url_encode(&serde_json::to_vec(&request).unwrap_or_default());

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
            "detail": "Payment is required to access this fortune.",
        })),
    )
        .into_response()
}

fn decode_credential(encoded: &str) -> Result<(String, u32), String> {
    let bytes = base64url_decode(encoded).map_err(|_| "invalid base64url".to_string())?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("invalid JSON: {e}"))?;
    let txid = value
        .pointer("/payload/txid")
        .and_then(|v| v.as_str())
        .ok_or("missing payload.txid")?
        .to_string();
    let output_index = value
        .pointer("/payload/outputIndex")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    Ok((txid, output_index))
}

fn base64url_encode(data: &[u8]) -> String {
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut encoder = Base64Encoder::new(&mut buf);
        let _ = encoder.write_all(data);
    }
    String::from_utf8(buf).unwrap_or_default()
}

fn base64url_decode(input: &str) -> Result<Vec<u8>, ()> {
    // Simple base64url decoder
    let input = input.replace('-', "+").replace('_', "/");
    let padded = match input.len() % 4 {
        2 => format!("{input}=="),
        3 => format!("{input}="),
        _ => input,
    };
    // Use a simple decode
    let table = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = Vec::new();
    let chars: Vec<u8> = padded.bytes().collect();
    for chunk in chars.chunks(4) {
        if chunk.len() < 4 {
            break;
        }
        let mut buf = [0u8; 4];
        for (i, &b) in chunk.iter().enumerate() {
            if b == b'=' {
                buf[i] = 0;
            } else {
                buf[i] = table.iter().position(|&t| t == b).ok_or(())? as u8;
            }
        }
        output.push((buf[0] << 2) | (buf[1] >> 4));
        if chunk[2] != b'=' {
            output.push((buf[1] << 4) | (buf[2] >> 2));
        }
        if chunk[3] != b'=' {
            output.push((buf[2] << 6) | buf[3]);
        }
    }
    Ok(output)
}

struct Base64Encoder<'a> {
    inner: &'a mut Vec<u8>,
}

impl<'a> Base64Encoder<'a> {
    fn new(inner: &'a mut Vec<u8>) -> Self {
        Self { inner }
    }
}

impl std::io::Write for Base64Encoder<'_> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let table = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut i = 0;
        while i + 2 < data.len() {
            let b0 = data[i];
            let b1 = data[i + 1];
            let b2 = data[i + 2];
            self.inner.push(table[(b0 >> 2) as usize]);
            self.inner
                .push(table[((b0 & 0x03) << 4 | b1 >> 4) as usize]);
            self.inner
                .push(table[((b1 & 0x0f) << 2 | b2 >> 6) as usize]);
            self.inner.push(table[(b2 & 0x3f) as usize]);
            i += 3;
        }
        let remaining = data.len() - i;
        if remaining == 2 {
            let b0 = data[i];
            let b1 = data[i + 1];
            self.inner.push(table[(b0 >> 2) as usize]);
            self.inner
                .push(table[((b0 & 0x03) << 4 | b1 >> 4) as usize]);
            self.inner.push(table[((b1 & 0x0f) << 2) as usize]);
        } else if remaining == 1 {
            let b0 = data[i];
            self.inner.push(table[(b0 >> 2) as usize]);
            self.inner.push(table[((b0 & 0x03) << 4) as usize]);
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn simple_uuid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{:016x}-{:04x}", now.as_nanos(), std::process::id())
}

fn chrono_now() -> String {
    // Simple ISO 8601 without chrono dependency
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{secs}")
}

fn chrono_future(seconds: u64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        + seconds;
    format!("{secs}")
}

fn rand_index(len: usize) -> usize {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as usize;
    nanos % len
}
