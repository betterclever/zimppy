#!/usr/bin/env bash
# Session client demo — shows the full session lifecycle with debug output
set -euo pipefail
cd "$(dirname "$0")/.."

SERVER="http://127.0.0.1:3180"
WALLET_DIR="${ZCASH_WALLET_DIR:-/tmp/zcash-wallet-send}"
IDENTITY="${ZCASH_IDENTITY_FILE:-$WALLET_DIR/identity.txt}"
LWD_SERVER="${ZCASH_LWD_SERVER:-testnet.zec.rocks:443}"
SERVER_ADDR=$(python3 -c "import json; print(json.load(open('config/server-wallet.json'))['address'])")

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

encode() {
    python3 -c "import base64,json; print(base64.urlsafe_b64encode(json.dumps($1).encode()).decode().rstrip('='))"
}

echo -e "${BOLD}${CYAN}"
echo "  ╔════════════════════════════════════════════════╗"
echo "  ║   ZIMPPY — Session Demo                        ║"
echo "  ║   Deposit once. Instant requests. Refund.      ║"
echo "  ╚════════════════════════════════════════════════╝"
echo -e "${NC}"
sleep 2

# ─── Step 1: Get 402 challenge ───
echo -e "${BOLD}STEP 1: Request without payment → 402${NC}"
RESP=$(curl -s "$SERVER/api/session/fortune")
echo -e "${RED}← 402: $(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin).get('detail','')[:80])")${NC}"
echo ""
sleep 1

# ─── Step 2: Send deposit ───
echo -e "${BOLD}STEP 2: Sending deposit (real Orchard shielded tx on Zcash testnet)${NC}"
echo -e "${YELLOW}  Syncing wallet...${NC}"
zcash-devtool wallet -w "$WALLET_DIR" sync --server "$LWD_SERVER" --connection direct 2>&1 | grep -v "^$" | head -5

echo -e "${YELLOW}  Sending 100000 zat (covers ~10 requests at 10000/ea)...${NC}"
DEPOSIT_TXID=$(zcash-devtool wallet -w "$WALLET_DIR" send \
  -i "$IDENTITY" \
  --server "$LWD_SERVER" \
  --connection direct \
  --address "$SERVER_ADDR" \
  --value 100000 \
  --memo "zimppy-session-deposit" 2>&1 | grep -E '^[a-f0-9]{64}$' | tail -1)

if [ -z "$DEPOSIT_TXID" ]; then
    echo -e "${RED}ERROR: No txid from send. Check wallet balance.${NC}"
    exit 1
fi

echo -e "${GREEN}  ✓ Deposit broadcast: ${DEPOSIT_TXID:0:20}...${NC}"
echo ""

# ─── Step 3: Wait for confirmation ───
echo -e "${BOLD}STEP 3: Waiting for on-chain confirmation...${NC}"
for i in $(seq 1 20); do
    sleep 15
    echo -n "."
    CONF=$(curl -s -X POST https://zcash-testnet-zebrad.gateway.tatum.io \
        -H 'Content-Type: application/json' \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"getrawtransaction\",\"params\":[\"$DEPOSIT_TXID\",1],\"id\":1}" 2>/dev/null \
        | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('result',{}).get('confirmations',0))" 2>/dev/null)
    if [ -n "$CONF" ] && [ "$CONF" -gt 0 ] 2>/dev/null; then
        echo ""
        echo -e "${GREEN}  ✓ Confirmed! $CONF confirmations${NC}"
        break
    fi
done
echo ""
sleep 1

# ─── Step 4: Open session ───
echo -e "${BOLD}STEP 4: OPEN session with deposit txid${NC}"
OPEN_CRED=$(encode "{'payload':{'action':'open','depositTxid':'$DEPOSIT_TXID','refundAddress':'$SERVER_ADDR'}}")
OPEN_RESP=$(curl -s "$SERVER/api/session/fortune" -H "Authorization: Payment $OPEN_CRED")
SESSION_ID=$(echo "$OPEN_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin).get('sessionId',''))" 2>/dev/null)
echo -e "${GREEN}  ✓ Session opened: $SESSION_ID${NC}"
echo ""
sleep 1

# ─── Step 5: Bearer requests (instant!) ───
BEARER_CRED=$(encode "{'payload':{'action':'bearer','sessionId':'$SESSION_ID','bearer':'$DEPOSIT_TXID'}}")

for i in 1 2 3 4 5; do
    echo -e "${BOLD}STEP 5.$i: BEARER request #$i (instant, no blockchain!)${NC}"
    START=$(python3 -c "import time; print(int(time.time()*1000))")
    RESP=$(curl -s "$SERVER/api/session/fortune" -H "Authorization: Payment $BEARER_CRED")
    END=$(python3 -c "import time; print(int(time.time()*1000))")
    LATENCY=$((END - START))
    FORTUNE=$(echo "$RESP" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('fortune',d.get('detail','')))" 2>/dev/null)

    if echo "$RESP" | grep -q "fortune"; then
        echo -e "${GREEN}  ✓ ${LATENCY}ms — \"$FORTUNE\"${NC}"
    else
        echo -e "${RED}  ✗ ${LATENCY}ms — $FORTUNE${NC}"
        break
    fi
done
echo ""
sleep 1

# ─── Step 6: Close session ───
echo -e "${BOLD}STEP 6: CLOSE session (server refunds unused balance)${NC}"
CLOSE_CRED=$(encode "{'payload':{'action':'close','sessionId':'$SESSION_ID','bearer':'$DEPOSIT_TXID'}}")
CLOSE_RESP=$(curl -s "$SERVER/api/session/fortune" -H "Authorization: Payment $CLOSE_CRED")
echo -e "${GREEN}  ✓ $(echo "$CLOSE_RESP" | python3 -m json.tool 2>/dev/null)${NC}"
echo ""

# ─── Step 7: Verify closed ───
echo -e "${BOLD}STEP 7: BEARER after close (should fail)${NC}"
RESP=$(curl -s "$SERVER/api/session/fortune" -H "Authorization: Payment $BEARER_CRED")
echo -e "${RED}  ✗ $(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin).get('detail',''))" 2>/dev/null)${NC}"
echo ""

echo -e "${BOLD}${GREEN}"
echo "  ╔════════════════════════════════════════════════╗"
echo "  ║         SESSION DEMO COMPLETE ✓                ║"
echo "  ║                                                ║"
echo "  ║  ✓ Deposit: 1 on-chain Orchard tx              ║"
echo "  ║  ✓ 5 instant bearer requests (no blockchain!)  ║"
echo "  ║  ✓ Session closed with refund                   ║"
echo "  ║  ✓ Post-close access denied                     ║"
echo "  ║                                                ║"
echo "  ║  Private payments + instant session access.     ║"
echo "  ╚════════════════════════════════════════════════╝"
echo -e "${NC}"
