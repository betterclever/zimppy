#!/usr/bin/env bash
# 🔒 Charge Demo — single shielded payment per request
#
# Layout: server (left) | CLI client (right)
set -euo pipefail
cd "$(dirname "$0")/../.."

echo "⚡ Building..."
cargo build --bin zimppy-rust-server 2>&1 | tail -2

pkill -f zimppy-rust-server 2>/dev/null || true
tmux kill-session -t charge-demo 2>/dev/null || true
sleep 1

# Left pane: server
tmux new-session -d -s charge-demo -x 200 -y 50
tmux send-keys -t charge-demo "PRICE_ZAT=10000 ./target/debug/zimppy-rust-server 2>&1" Enter

# Right pane: CLI
tmux split-window -h -t charge-demo
sleep 2
tmux send-keys -t charge-demo "echo '🔒 Zimppy Charge Demo'" Enter
tmux send-keys -t charge-demo "echo ''" Enter
tmux send-keys -t charge-demo "echo 'Sending a single paid request to /api/fortune...'" Enter
tmux send-keys -t charge-demo "echo ''" Enter
tmux send-keys -t charge-demo "npx zimppy request http://localhost:3180/api/fortune" Enter

tmux attach -t charge-demo
