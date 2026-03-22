//! SSE Streamed Payments — pay-per-token metered streaming.
//!
//! Wraps an async stream of content chunks and deducts from a session
//! balance per chunk. Emits `payment-need-topup` when balance is
//! exhausted and waits for a topUp before resuming.

use crate::session::{SessionError, ZcashSessionMethod};

/// Options for serving a metered SSE stream.
pub struct ServeStreamOptions {
    pub session_id: String,
    pub tick_cost_zat: u64,
    pub top_up_timeout_ms: u64,
    pub poll_interval_ms: u64,
}

impl Default for ServeStreamOptions {
    fn default() -> Self {
        Self {
            session_id: String::new(),
            tick_cost_zat: 1000,
            top_up_timeout_ms: 300_000,
            poll_interval_ms: 1_000,
        }
    }
}

/// SSE event types for streamed payments.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "event", content = "data")]
pub enum SseEvent {
    /// Content chunk delivered to the client.
    Message(String),
    /// Balance exhausted — client must send a topUp.
    PaymentNeedVoucher {
        session_id: String,
        required_amount: u64,
        current_balance: u64,
    },
    /// Stream complete — final receipt.
    PaymentReceipt {
        session_id: String,
        total_spent: u64,
        total_chunks: u64,
    },
    /// Error during streaming.
    Error(String),
}

impl SseEvent {
    /// Format as an SSE text frame.
    pub fn to_sse_string(&self) -> String {
        match self {
            Self::Message(data) => format!("event: message\ndata: {data}\n\n"),
            Self::PaymentNeedVoucher { session_id, required_amount, current_balance } => {
                let json = serde_json::json!({
                    "sessionId": session_id,
                    "requiredAmount": required_amount,
                    "currentBalance": current_balance,
                });
                format!("event: payment-need-topup\ndata: {json}\n\n")
            }
            Self::PaymentReceipt { session_id, total_spent, total_chunks } => {
                let json = serde_json::json!({
                    "sessionId": session_id,
                    "totalSpent": total_spent,
                    "totalChunks": total_chunks,
                });
                format!("event: payment-receipt\ndata: {json}\n\n")
            }
            Self::Error(msg) => {
                let json = serde_json::json!({ "error": msg });
                format!("event: error\ndata: {json}\n\n")
            }
        }
    }
}

/// Serve a metered SSE stream, deducting from session balance per chunk.
///
/// Yields `SseEvent` items. The caller converts these to HTTP SSE responses.
///
/// When balance is exhausted, yields `PaymentNeedVoucher` and polls
/// until the client tops up or the timeout expires.
pub async fn serve_stream<S>(
    session: &ZcashSessionMethod,
    options: &ServeStreamOptions,
    mut generate: S,
) -> Vec<SseEvent>
where
    S: futures_lite::Stream<Item = String> + Unpin,
{
    use futures_lite::StreamExt;

    let mut events = Vec::new();
    let mut total_spent: u64 = 0;
    let mut total_chunks: u64 = 0;

    while let Some(chunk) = generate.next().await {
        // Try to deduct from session balance via bearer
        match try_deduct(session, &options.session_id, options.tick_cost_zat) {
            Ok(()) => {
                total_spent += options.tick_cost_zat;
                total_chunks += 1;
                events.push(SseEvent::Message(chunk));
            }
            Err(_) => {
                // Balance exhausted
                let balance = get_balance(session, &options.session_id);
                events.push(SseEvent::PaymentNeedVoucher {
                    session_id: options.session_id.clone(),
                    required_amount: options.tick_cost_zat,
                    current_balance: balance,
                });

                // Poll for topUp
                let deadline = std::time::Instant::now()
                    + std::time::Duration::from_millis(options.top_up_timeout_ms);
                let mut funded = false;

                while std::time::Instant::now() < deadline {
                    tokio::time::sleep(std::time::Duration::from_millis(options.poll_interval_ms)).await;
                    if try_deduct(session, &options.session_id, options.tick_cost_zat).is_ok() {
                        total_spent += options.tick_cost_zat;
                        total_chunks += 1;
                        funded = true;
                        break;
                    }
                }

                if funded {
                    events.push(SseEvent::Message(chunk));
                } else {
                    events.push(SseEvent::Error("topUp timeout".to_string()));
                    break;
                }
            }
        }
    }

    events.push(SseEvent::PaymentReceipt {
        session_id: options.session_id.clone(),
        total_spent,
        total_chunks,
    });

    events
}

/// Try to deduct tick_cost from the session balance.
fn try_deduct(
    session: &ZcashSessionMethod,
    session_id: &str,
    amount: u64,
) -> Result<(), SessionError> {
    session.deduct(session_id, amount)?;
    Ok(())
}

/// Get current session balance.
fn get_balance(session: &ZcashSessionMethod, session_id: &str) -> u64 {
    session
        .get_session(session_id)
        .map(|s| s.deposit_amount_zat.saturating_sub(s.spent_zat))
        .unwrap_or(0)
}
