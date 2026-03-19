#!/usr/bin/env bash
# Full MPP demo using the TypeScript HTTP merchant server
set -euo pipefail
cd "$(dirname "$0")/.."

echo "Building..."
cargo build --workspace >/dev/null
npx tsc --noEmit --project tsconfig.json >/dev/null

pkill -f zimppy-core-server 2>/dev/null || true
pkill -f 'tsx apps/ts-server/src/server.ts' 2>/dev/null || true
tmux kill-session -t mpp-demo-ts 2>/dev/null || true
sleep 1

tmux new-session -d -s mpp-demo-ts -x 220 -y 60

tmux send-keys -t mpp-demo-ts:0.0 "cd /Users/betterclever/newprojects/experiments/zimppy && echo '=== ZIMPPY-CORE VERIFICATION SERVER ===' && PORT=3181 RUST_LOG=debug ./target/debug/zimppy-core-server" Enter
tmux split-window -h -t mpp-demo-ts:0.0
tmux send-keys -t mpp-demo-ts:0.1 "cd /Users/betterclever/newprojects/experiments/zimppy && echo '=== ZIMPPY TS HTTP SERVER ===' && PRICE_ZAT=42000 PORT=3180 npx tsx apps/ts-server/src/server.ts" Enter
tmux split-window -v -t mpp-demo-ts:0.1
tmux send-keys -t mpp-demo-ts:0.2 "cd /Users/betterclever/newprojects/experiments/zimppy && bash scripts/live-e2e.sh" Enter

tmux select-layout -t mpp-demo-ts tiled
tmux attach -t mpp-demo-ts
