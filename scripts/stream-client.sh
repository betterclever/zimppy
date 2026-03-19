#!/usr/bin/env bash
# SSE Streaming client — opens session, streams fortune tokens word-by-word
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
echo "  ║   ZIMPPY — SSE Streaming Demo                  ║"
echo "  ║   Pay-per-token fortune streaming               ║"
echo "  ╚════════════════════════════════════════════════╝"
echo -e "${NC}"
sleep 2

# ─── Step 1: Deposit ───
echo -e "${BOLD}STEP 1: Sending deposit for streaming session${NC}"
echo -e "${YELLOW}  Syncing wallet...${NC}"
zcash-devtool wallet -w "$WALLET_DIR" sync --server "$LWD_SERVER" --connection direct 2>&1 | grep -v "^$" | head -3

echo -e "${YELLOW}  Sending 50000 zat deposit (covers ~50 words at 1000 zat/word)...${NC}"
DEPOSIT_TXID=$(zcash-devtool wallet -w "$WALLET_DIR" send \
  -i "$IDENTITY" \
  --server "$LWD_SERVER" \
  --connection direct \
  --address "$SERVER_ADDR" \
  --value 50000 \
  --memo "zimppy-stream-deposit" 2>&1 | grep -E '^[a-f0-9]{64}$' | tail -1)

if [ -z "$DEPOSIT_TXID" ]; then
    echo -e "${RED}ERROR: No txid. Check wallet balance.${NC}"
    exit 1
fi
echo -e "${GREEN}  ✓ Deposit: ${DEPOSIT_TXID:0:20}...${NC}"
echo ""

# ─── Step 2: Wait for confirmation ───
echo -e "${BOLD}STEP 2: Waiting for confirmation...${NC}"
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

# ─── Step 3: Open session ───
echo -e "${BOLD}STEP 3: Opening session${NC}"
OPEN_CRED=$(encode "{'payload':{'action':'open','depositTxid':'$DEPOSIT_TXID','refundAddress':'$SERVER_ADDR'}}")
OPEN_RESP=$(curl -s "$SERVER/api/session/fortune" -H "Authorization: Payment $OPEN_CRED")
SESSION_ID=$(echo "$OPEN_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin).get('sessionId',''))" 2>/dev/null)
echo -e "${GREEN}  ✓ Session: $SESSION_ID${NC}"
echo ""

# ─── Step 4: Stream! ───
echo -e "${BOLD}STEP 4: Streaming fortune (pay-per-word via SSE)${NC}"
echo -e "${YELLOW}  Each word costs 1000 zat. Watching tokens arrive...${NC}"
echo ""

BEARER_CRED=$(encode "{'payload':{'action':'bearer','sessionId':'$SESSION_ID','bearer':'$DEPOSIT_TXID'}}")

# Fetch the SSE stream
echo -e "${CYAN}  ──── Stream Start ────${NC}"
curl -sN "$SERVER/api/stream/fortune" -H "Authorization: Payment $BEARER_CRED" | while IFS= read -r line; do
    if [[ "$line" == "event: message" ]]; then
        read -r data_line
        TOKEN=$(echo "${data_line#data: }" | python3 -c "import sys,json; print(json.load(sys.stdin).get('token',''))" 2>/dev/null)
        REMAINING=$(echo "${data_line#data: }" | python3 -c "import sys,json; print(json.load(sys.stdin).get('remaining',''))" 2>/dev/null)
        echo -e "  ${GREEN}▸ \"$TOKEN\"${NC}  ${YELLOW}(${REMAINING} zat remaining)${NC}"
    elif [[ "$line" == "event: payment-need-voucher" ]]; then
        read -r data_line
        echo -e "  ${RED}⚠ Balance exhausted! Need topUp to continue.${NC}"
    elif [[ "$line" == "event: payment-receipt" ]]; then
        read -r data_line
        SPENT=$(echo "${data_line#data: }" | python3 -c "import sys,json; print(json.load(sys.stdin).get('totalSpent',0))" 2>/dev/null)
        CHUNKS=$(echo "${data_line#data: }" | python3 -c "import sys,json; print(json.load(sys.stdin).get('totalChunks',0))" 2>/dev/null)
        echo -e "  ${CYAN}──── Stream End ────${NC}"
        echo -e "  ${GREEN}✓ ${CHUNKS} words streamed, ${SPENT} zat spent${NC}"
    fi
done

echo ""

# ─── Step 5: Close session ───
echo -e "${BOLD}STEP 5: Closing session${NC}"
CLOSE_CRED=$(encode "{'payload':{'action':'close','sessionId':'$SESSION_ID','bearer':'$DEPOSIT_TXID'}}")
CLOSE_RESP=$(curl -s "$SERVER/api/session/fortune" -H "Authorization: Payment $CLOSE_CRED")
echo -e "${GREEN}  ✓ $(echo "$CLOSE_RESP" | python3 -c "import sys,json; d=json.load(sys.stdin); print(f'Closed. Action: {d.get(\"action\")}')" 2>/dev/null)${NC}"
echo ""

echo -e "${BOLD}${GREEN}"
echo "  ╔════════════════════════════════════════════════╗"
echo "  ║       STREAMING DEMO COMPLETE ✓                ║"
echo "  ║                                                ║"
echo "  ║  ✓ Shielded deposit (1 on-chain tx)            ║"
echo "  ║  ✓ Fortune streamed word-by-word via SSE        ║"
echo "  ║  ✓ 1000 zat charged per word (real billing)     ║"
echo "  ║  ✓ Session closed with refund                   ║"
echo "  ║                                                ║"
echo "  ║  Private. Metered. Streamed. Zcash.             ║"
echo "  ╚════════════════════════════════════════════════╝"
echo -e "${NC}"
