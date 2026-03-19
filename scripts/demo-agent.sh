#!/usr/bin/env bash
# AI Agent Demo — MCP client auto-pays for tools with real Zcash
set -euo pipefail
cd "$(dirname "$0")/.."

echo "Building..."
cargo build --bin zimppy-rust-server 2>&1 | tail -2

pkill -f zimppy-rust-server 2>/dev/null || true
tmux kill-session -t agent-demo 2>/dev/null || true
sleep 1

# Pane 0: MCP Server logs (stderr from the MCP server spawned by the client)
tmux new-session -d -s agent-demo -x 200 -y 50
tmux send-keys -t agent-demo "echo '=== AI AGENT DEMO ===' && echo 'MCP server will start when agent connects (via stdio)' && echo 'Rust MPP server needed for shielded verification...' && PRICE_ZAT=10000 ./target/debug/zimppy-rust-server 2>&1" Enter

# Pane 1: AI Agent (MCP client with auto-pay)
tmux split-window -h -t agent-demo
sleep 3
tmux send-keys -t agent-demo "echo '=== AI AGENT — Auto-Pay Demo ===' && echo '' && echo 'The agent will:' && echo '  1. Connect to MCP server' && echo '  2. Call get_zcash_info (paid tool, 10000 zat)' && echo '  3. Get 402 Payment Required' && echo '  4. Auto-send ZEC on Zcash testnet' && echo '  5. Wait for confirmation' && echo '  6. Retry with credential' && echo '  7. Get the tool result + receipt' && echo '' && echo 'Starting in 3 seconds...' && sleep 3 && npx tsx apps/demo/mcp-pay.ts 2>&1" Enter

tmux select-layout -t agent-demo main-horizontal
echo "Attach: tmux attach -t agent-demo"
