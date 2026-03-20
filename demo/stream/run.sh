#!/usr/bin/env bash
# 📡 Stream Demo — SSE pay-per-word fortune streaming
#
# Layout: server (left) | CLI client (right)
set -euo pipefail
cd "$(dirname "$0")/../.."

echo "⚡ Building..."
cargo build --bin zimppy-rust-server 2>&1 | tail -2

pkill -f zimppy-rust-server 2>/dev/null || true
tmux kill-session -t stream-demo 2>/dev/null || true
sleep 1

# Clear any stale session
rm -f ~/.zimppy/session.json

# Left pane: server
tmux new-session -d -s stream-demo -x 200 -y 50
tmux send-keys -t stream-demo "PRICE_ZAT=10000 ./target/debug/zimppy-rust-server 2>&1" Enter

# Right pane: CLI
tmux split-window -h -t stream-demo
sleep 2
tmux send-keys -t stream-demo "echo '📡 Zimppy Stream Demo'" Enter
tmux send-keys -t stream-demo "echo ''" Enter
tmux send-keys -t stream-demo "echo 'Streaming a fortune word-by-word (1000 zat/word)...'" Enter
tmux send-keys -t stream-demo "echo ''" Enter
tmux send-keys -t stream-demo "npx zimppy request http://localhost:3180/api/stream/fortune" Enter

tmux attach -t stream-demo
