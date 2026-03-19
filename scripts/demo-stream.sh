#!/usr/bin/env bash
# SSE Streaming demo — pay-per-token fortune streaming
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --bin zimppy-rust-server 2>&1 | tail -2

pkill -f zimppy-rust-server 2>/dev/null || true
tmux kill-session -t stream-demo 2>/dev/null || true
sleep 1

# Pane 0: Server
tmux new-session -d -s stream-demo -x 200 -y 50
tmux send-keys -t stream-demo "PRICE_ZAT=5000 ./target/debug/zimppy-rust-server 2>&1" Enter

# Pane 1: Client
tmux split-window -h -t stream-demo
sleep 3
tmux send-keys -t stream-demo "bash scripts/stream-client.sh" Enter

tmux select-layout -t stream-demo main-horizontal
echo "Attach: tmux attach -t stream-demo"
