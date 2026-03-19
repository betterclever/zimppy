#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

MCP_SOCKET_PORT=${MCP_SOCKET_PORT:-8765}
TX_TRACK_FILE=/tmp/zimppy-last-txid

printf '' > "$TX_TRACK_FILE"

pkill -f 'tsx apps/mcp-server/src/socket-server.ts' 2>/dev/null || true
pkill -f 'tsx apps/demo/mcp-repl.ts' 2>/dev/null || true
tmux kill-session -t mpp-demo-mcp 2>/dev/null || true

tmux new-session -d -s mpp-demo-mcp -x 240 -y 70
tmux send-keys -t mpp-demo-mcp:0.0 "cd /Users/betterclever/newprojects/experiments/zimppy && npx tsx apps/demo/block-progress.ts" Enter
tmux split-window -v -t mpp-demo-mcp:0.0
tmux send-keys -t mpp-demo-mcp:0.1 "cd /Users/betterclever/newprojects/experiments/zimppy && MCP_SOCKET_PORT=$MCP_SOCKET_PORT npx tsx apps/mcp-server/src/socket-server.ts" Enter
tmux split-window -h -t mpp-demo-mcp:0.1
tmux send-keys -t mpp-demo-mcp:0.2 "cd /Users/betterclever/newprojects/experiments/zimppy && MCP_SOCKET_PORT=$MCP_SOCKET_PORT MCP_DEMO_AUTORUN=info npx tsx apps/demo/mcp-repl.ts" Enter
tmux select-layout -t mpp-demo-mcp tiled
tmux attach -t mpp-demo-mcp
