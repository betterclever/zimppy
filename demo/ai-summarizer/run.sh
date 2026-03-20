#!/usr/bin/env bash
# 🤖 AI Summarizer Demo — OpenCode agent pays ZEC for AI summaries
#
# Layout: AI server (left) | OpenCode agent (right)
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

# Left pane: AI summarizer server
tmux new-session -d -s ai-demo -x 200 -y 50
tmux send-keys -t ai-demo "PRICE_ZAT=10000 PORT=3181 ./target/debug/zimppy-ai-server 2>&1" Enter

# Right pane: OpenCode agent
tmux split-window -h -t ai-demo
sleep 2
tmux send-keys -t ai-demo "cd $(pwd)/demo/opencode && opencode" Enter

tmux attach -t ai-demo
