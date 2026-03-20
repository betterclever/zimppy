#!/usr/bin/env bash
# 🎫 Session Demo — deposit once, multiple instant requests, close with refund
#
# Layout: server (left) | CLI client (right)
set -euo pipefail
cd "$(dirname "$0")/../.."

echo "⚡ Building..."
cargo build --bin zimppy-rust-server 2>&1 | tail -2

pkill -f zimppy-rust-server 2>/dev/null || true
tmux kill-session -t session-demo 2>/dev/null || true
sleep 1

# Clear any stale session
rm -f ~/.zimppy/session.json

# Left pane: server
tmux new-session -d -s session-demo -x 200 -y 50
tmux send-keys -t session-demo "PRICE_ZAT=10000 ./target/debug/zimppy-rust-server 2>&1" Enter

# Right pane: CLI
tmux split-window -h -t session-demo
sleep 2

# First request opens session with 10x deposit
tmux send-keys -t session-demo "echo '🎫 Zimppy Session Demo'" Enter
tmux send-keys -t session-demo "echo ''" Enter
tmux send-keys -t session-demo "echo '1️⃣  First request — opens session (deposits 10x, waits for block)'" Enter
tmux send-keys -t session-demo "npx zimppy request http://localhost:3180/api/session/fortune" Enter

# Queue up instant bearer requests (will run after first completes)
tmux send-keys -t session-demo "echo ''" Enter
tmux send-keys -t session-demo "echo '2️⃣  Second request — instant via bearer token'" Enter
tmux send-keys -t session-demo "npx zimppy request http://localhost:3180/api/session/fortune" Enter

tmux send-keys -t session-demo "echo ''" Enter
tmux send-keys -t session-demo "echo '3️⃣  Third request — instant via bearer token'" Enter
tmux send-keys -t session-demo "npx zimppy request http://localhost:3180/api/session/fortune" Enter

# Close session and get refund
tmux send-keys -t session-demo "echo ''" Enter
tmux send-keys -t session-demo "echo '🔚 Closing session — remaining balance refunded'" Enter
tmux send-keys -t session-demo "npx zimppy session close" Enter

tmux attach -t session-demo
