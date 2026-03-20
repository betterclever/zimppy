#!/usr/bin/env bash
# Launch OpenCode AI agent in Docker with Zimppy paid tools
# Pre-opens a session so the agent can use bearer tokens (no wallet in Docker)
set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== Zimppy AI Agent Demo ==="
echo ""

# 1. Build
echo "Building..."
cargo build --bin zimppy-rust-server 2>&1 | tail -2

# 2. Start MPP server
pkill -f zimppy-rust-server 2>/dev/null || true
sleep 1
PRICE_ZAT=5000 ./target/debug/zimppy-rust-server 2>/tmp/mpp-agent.log &
sleep 2
echo "MPP server running on :3180"

# 3. Pre-open a session with a real deposit
echo ""
echo "Pre-funding agent session..."

# Get a challenge first (to trigger deposit verification)
# Use an existing confirmed deposit txid from our wallet
# Pick the most recent E2E tx
DEPOSIT_TXID="${DEPOSIT_TXID:-}"

if [ -z "$DEPOSIT_TXID" ]; then
  echo "  Sending deposit of 200000 zat..."
  WALLET_DIR="${ZCASH_WALLET_DIR:-/tmp/zcash-wallet-send}"
  IDENTITY="${ZCASH_IDENTITY_FILE:-$WALLET_DIR/identity.txt}"
  LWD_SERVER="${ZCASH_LWD_SERVER:-testnet.zec.rocks:443}"
  SERVER_ADDR=$(python3 -c "import json; print(json.load(open('config/server-wallet.json'))['address'])")

  zcash-devtool wallet -w "$WALLET_DIR" sync --server "$LWD_SERVER" --connection direct 2>&1 | grep -v "^$" | head -3
  DEPOSIT_TXID=$(zcash-devtool wallet -w "$WALLET_DIR" send \
    -i "$IDENTITY" \
    --server "$LWD_SERVER" \
    --connection direct \
    --address "$SERVER_ADDR" \
    --value 200000 \
    --memo "zimppy-agent-session" 2>&1 | grep -E '^[a-f0-9]{64}$' | tail -1)

  echo "  Deposit txid: ${DEPOSIT_TXID:0:20}..."
  echo "  Waiting for confirmation..."
  for i in $(seq 1 20); do
    sleep 15
    echo -n "."
    CONF=$(curl -s -X POST https://zcash-testnet-zebrad.gateway.tatum.io \
      -H 'Content-Type: application/json' \
      -d "{\"jsonrpc\":\"2.0\",\"method\":\"getrawtransaction\",\"params\":[\"$DEPOSIT_TXID\",1],\"id\":1}" 2>/dev/null \
      | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('result',{}).get('confirmations',0))" 2>/dev/null)
    if [ -n "$CONF" ] && [ "$CONF" -gt 0 ] 2>/dev/null; then
      echo ""
      echo "  Confirmed!"
      break
    fi
  done
fi

# Open session
echo "  Opening session..."
OPEN_CRED=$(python3 -c "import base64,json; print(base64.urlsafe_b64encode(json.dumps({'payload':{'action':'open','depositTxid':'$DEPOSIT_TXID','refundAddress':'utest1dummy'}}).encode()).decode().rstrip('='))")
OPEN_RESP=$(curl -s http://127.0.0.1:3180/api/session/fortune -H "Authorization: Payment $OPEN_CRED")
SESSION_ID=$(echo "$OPEN_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin).get('sessionId',''))" 2>/dev/null)

if [ -z "$SESSION_ID" ]; then
  echo "  ERROR: Failed to open session"
  echo "  $OPEN_RESP"
  exit 1
fi

echo "  Session: $SESSION_ID"
echo "  Bearer: ${DEPOSIT_TXID:0:20}..."
echo ""

# 4. Build Docker image if needed
echo "Building Docker image..."
docker build -t zimppy-opencode -f docker/opencode/Dockerfile . 2>&1 | tail -3

# 5. Launch OpenCode
echo ""
echo "=== Launching OpenCode ==="
echo "  Model: GPT 5.4 via VibeProxy"
echo "  Session: $SESSION_ID (pre-funded with 200000 zat)"
echo ""
echo "  The agent has access to paid Zcash tools."
echo "  Try: 'what tools do you have?'"
echo "  Or:  'get me some zcash network info'"
echo ""

docker run -it --rm \
  --add-host=host.docker.internal:host-gateway \
  -e ZIMPPY_SESSION_ID="$SESSION_ID" \
  -e ZIMPPY_BEARER="$DEPOSIT_TXID" \
  zimppy-opencode
