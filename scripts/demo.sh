#!/usr/bin/env bash
# Full MPP demo with debug logs in tmux panes
#
# Layout:
#   ┌──────────────────────┬──────────────────────┐
#   │  Crypto Server       │  MPP Server           │
#   │  (zimppy-core)       │  (rust-server)        │
#   ├──────────────────────┴──────────────────────┤
#   │              Client (demo script)             │
#   └───────────────────────────────────────────────┘

set -e
cd "$(dirname "$0")/.."

# Build first
echo "Building..."
cargo build --workspace 2>&1 | tail -3

# Kill existing
pkill -f zimppy-core-server 2>/dev/null || true
pkill -f zimppy-rust-server 2>/dev/null || true
tmux kill-session -t mpp-demo 2>/dev/null || true
sleep 1

RECIPIENT="t2UNzUUx8mWBCRYPRezvA363EYXyEpHokyi"
RPC="https://zcash-testnet-zebrad.gateway.tatum.io"
REAL_TXID="f37e9f691fffb635de0999491d906ee85ba40cd36dae9f6e5911a8277d7c5f75"

# Create tmux session with panes
tmux new-session -d -s mpp-demo -x 200 -y 50

# Pane 0: Crypto server (top-left)
tmux send-keys -t mpp-demo "echo '=== ZIMPPY-CORE VERIFICATION SERVER ===' && PORT=3181 RUST_LOG=debug ./target/debug/zimppy-core-server" Enter

# Split horizontally: Pane 1 (top-right) — MPP server
tmux split-window -h -t mpp-demo
tmux send-keys -t mpp-demo "sleep 2 && echo '=== ZIMPPY RUST MPP SERVER ===' && PRICE_ZAT=10000 PORT=3180 ZCASH_RECIPIENT=$RECIPIENT RUST_LOG=debug ./target/debug/zimppy-rust-server" Enter

# Split vertically: Pane 2 (bottom) — Client demo
tmux split-window -v -t mpp-demo
tmux send-keys -t mpp-demo "sleep 4 && bash scripts/demo-client.sh" Enter

tmux select-layout -t mpp-demo main-horizontal
tmux attach -t mpp-demo
