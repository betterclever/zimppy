#!/usr/bin/env bash
# Session E2E demo — deposit once, multiple instant requests, close with refund
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --bin zimppy-rust-server 2>&1 | tail -2

pkill -f zimppy-rust-server 2>/dev/null || true
tmux kill-session -t session-demo 2>/dev/null || true
sleep 1

# Pane 0: Server
tmux new-session -d -s session-demo -x 200 -y 50
tmux send-keys -t session-demo "PRICE_ZAT=10000 ZCASH_WALLET_DIR=/tmp/zcash-wallet-server ZCASH_LWD_SERVER=testnet.zec.rocks:443 ./target/debug/zimppy-rust-server 2>&1" Enter

# Pane 1: Client
tmux split-window -h -t session-demo
sleep 3
tmux send-keys -t session-demo "bash scripts/session-client.sh" Enter

tmux select-layout -t session-demo main-horizontal
echo "Attach: tmux attach -t session-demo"
