#!/usr/bin/env bash
# 🤖 AI Summarizer Demo — pay ZEC to get an AI-powered document summary
#
# Layout: AI server (left) | CLI client (right)
#
# Requires: VibeProxy running on localhost:8317
set -euo pipefail
cd "$(dirname "$0")/../.."

echo "⚡ Building..."
cargo build --bin zimppy-ai-server 2>&1 | tail -2

pkill -f zimppy-ai-server 2>/dev/null || true
tmux kill-session -t ai-demo 2>/dev/null || true
sleep 1

# Clear stale session
rm -f ~/.zimppy/session.json

# Left pane: AI server
tmux new-session -d -s ai-demo -x 200 -y 50
tmux send-keys -t ai-demo "PRICE_ZAT=50000 PORT=3181 ./target/debug/zimppy-ai-server 2>&1" Enter

# Right pane: CLI
tmux split-window -h -t ai-demo
sleep 2

tmux send-keys -t ai-demo "echo '🤖 Zimppy AI Summarizer Demo'" Enter
tmux send-keys -t ai-demo "echo ''" Enter
tmux send-keys -t ai-demo "echo 'Sending a document for AI summarization (50,000 zat)...'" Enter
tmux send-keys -t ai-demo "echo ''" Enter
tmux send-keys -t ai-demo "npx zimppy request -X POST --json '{\"text\": \"The Machine Payments Protocol (MPP) enables AI agents to autonomously pay for API services using HTTP 402 Payment Required responses. When an agent makes a request to a paid endpoint, the server responds with a 402 status code and a WWW-Authenticate header containing a payment challenge. The challenge includes the amount, recipient address, and a cryptographic memo that binds the payment to the specific request. The agent then sends the payment on-chain, waits for confirmation, and retries the request with an Authorization header containing the transaction ID and challenge ID. The server verifies the payment by checking the blockchain and serves the content. MPP supports three payment intents: charge (one-time payments), session (prepaid balance with bearer tokens), and streaming (pay-per-token via Server-Sent Events). The protocol is designed to be blockchain-agnostic, with implementations for Solana and now Zcash via the zimppy project.\"}' http://localhost:3181/api/summarize" Enter

tmux attach -t ai-demo
