#!/usr/bin/env bash
# AI Agent CLI Demo — agent autonomously pays for tools with Zcash
set -euo pipefail
cd "$(dirname "$0")/.."

QUERY="${1:-What is Zcash?}"

tmux kill-session -t agent-demo 2>/dev/null || true

tmux new-session -d -s agent-demo -x 200 -y 50
tmux send-keys -t agent-demo "npx tsx apps/demo/agent.ts \"$QUERY\"" Enter

echo "Attach: tmux attach -t agent-demo"
echo "Query: $QUERY"
