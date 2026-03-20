#!/usr/bin/env bash
# Launch OpenCode AI agent in Docker with Zimppy MCP server
set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== Building zimppy MPP server ==="
cargo build --bin zimppy-rust-server 2>&1 | tail -2

echo "=== Building OpenCode Docker image ==="
docker build -t zimppy-opencode -f docker/opencode/Dockerfile . 2>&1 | tail -5

echo "=== Starting MPP server (host) ==="
pkill -f zimppy-rust-server 2>/dev/null || true
sleep 1
PRICE_ZAT=10000 ./target/debug/zimppy-rust-server 2>/tmp/mpp-server-agent.log &
sleep 2
echo "  MPP server running on :3180"

echo ""
echo "=== Launching OpenCode in Docker ==="
echo "  Model: Claude Sonnet 4.6 via VibeProxy (host:8317)"
echo "  MCP: zimppy (paid tools with Zcash)"
echo ""
echo "  Try: 'use the zimppy tools to get zcash network info'"
echo "  Or:  'what is the weather in Tokyo? use zimppy'"
echo ""

docker run -it --rm \
  --add-host=host.docker.internal:host-gateway \
  -v /tmp/mpp-server-agent.log:/tmp/mpp-server.log:ro \
  zimppy-opencode
