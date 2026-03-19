use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use zimppy_core::{ConsumedTxids, TransparentVerifyRequest, ZebradRpc, verify_transparent};

#[cfg(feature = "shielded")]
use zimppy_core::shielded::{verify_shielded, ShieldedVerifyRequest};

struct AppState {
    rpc: ZebradRpc,
    consumed: ConsumedTxids,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VerifyTransparentBody {
    txid: String,
    output_index: u32,
    expected_address: String,
    expected_amount_zat: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VerifyShieldedBody {
    txid: String,
    ivk_hex: String,
    expected_challenge_id: String,
    expected_amount_zat: String,
}

#[derive(serde::Serialize)]
struct ErrorResponse {
    error: String,
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({
        "service": "zimppy-core",
        "status": "ok",
    }))
}

async fn verify_transparent_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<VerifyTransparentBody>,
) -> impl IntoResponse {
    eprintln!("[verify/transparent] txid={} addr={} amount={}", body.txid, body.expected_address, body.expected_amount_zat);

    let amount_zat: u64 = match body.expected_amount_zat.parse() {
        Ok(v) => v,
        Err(_) => {
            eprintln!("[verify/transparent] ERROR: invalid amount");
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::to_value(ErrorResponse {
                    error: "invalid expectedAmountZat: must be a numeric string".to_string(),
                })
                .unwrap_or_default()),
            )
                .into_response();
        }
    };

    let req = TransparentVerifyRequest {
        txid: body.txid,
        output_index: body.output_index,
        expected_address: body.expected_address,
        expected_amount_zat: amount_zat,
    };

    match verify_transparent(&state.rpc, &req, &state.consumed).await {
        Ok(result) => {
            eprintln!("[verify/transparent] result: verified={} amount={}", result.verified, result.observed_amount_zat);
            (StatusCode::OK, Json(serde_json::to_value(result).unwrap_or_default())).into_response()
        }
        Err(e) => {
            eprintln!("[verify/transparent] ERROR: {e}");
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::to_value(ErrorResponse {
                    error: e.to_string(),
                })
                .unwrap_or_default()),
            )
                .into_response()
        }
    }
}

#[cfg(feature = "shielded")]
async fn verify_shielded_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<VerifyShieldedBody>,
) -> impl IntoResponse {
    eprintln!("[verify/shielded] txid={} challenge={} amount={}", body.txid, body.expected_challenge_id, body.expected_amount_zat);

    let amount_zat: u64 = match body.expected_amount_zat.parse() {
        Ok(v) => v,
        Err(_) => {
            eprintln!("[verify/shielded] ERROR: invalid amount");
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::to_value(ErrorResponse {
                    error: "invalid expectedAmountZat: must be a numeric string".to_string(),
                })
                .unwrap_or_default()),
            )
                .into_response();
        }
    };

    let req = ShieldedVerifyRequest {
        txid: body.txid,
        ivk_bytes_hex: body.ivk_hex,
        expected_challenge_id: body.expected_challenge_id,
        expected_amount_zat: amount_zat,
    };

    match verify_shielded(&state.rpc, &req, &state.consumed).await {
        Ok(result) => {
            eprintln!("[verify/shielded] result: verified={} decrypted={} amount={} memo_matched={}",
                result.verified, result.outputs_decrypted, result.observed_amount_zat, result.memo_matched);
            (StatusCode::OK, Json(serde_json::to_value(result).unwrap_or_default())).into_response()
        }
        Err(e) => {
            eprintln!("[verify/shielded] ERROR: {e}");
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::to_value(ErrorResponse {
                    error: e.to_string(),
                })
                .unwrap_or_default()),
            )
                .into_response()
        }
    }
}

#[tokio::main]
async fn main() {
    let rpc_endpoint = std::env::var("ZEBRAD_RPC_ENDPOINT")
        .unwrap_or_else(|_| "https://zcash-testnet-zebrad.gateway.tatum.io".to_string());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3181);

    let state = Arc::new(AppState {
        rpc: ZebradRpc::new(&rpc_endpoint),
        consumed: ConsumedTxids::new(),
    });

    let mut app = Router::new()
        .route("/health", get(health))
        .route("/verify/transparent", post(verify_transparent_handler));

    #[cfg(feature = "shielded")]
    {
        app = app.route("/verify/shielded", post(verify_shielded_handler));
    }

    let app = app.with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
        .await
        .unwrap_or_else(|e| panic!("failed to bind port {port}: {e}"));

    eprintln!("zimppy-core server listening on http://127.0.0.1:{port}");
    eprintln!("  RPC endpoint: {rpc_endpoint}");
    eprintln!("  shielded verification: {}", cfg!(feature = "shielded"));

    axum::serve(listener, app)
        .await
        .unwrap_or_else(|e| panic!("server error: {e}"));
}
