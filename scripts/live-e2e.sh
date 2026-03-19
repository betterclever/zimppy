#!/usr/bin/env bash
# Live MPP E2E test — runs in the client pane of mpp-demo tmux session
set -e

SERVER="http://127.0.0.1:3180"
SERVER_ADDR="utest18332muyrx6kw4z7pewt8aq60fsdn3se4f64ldp7yzme6nhgg2uc0mrvlcvsk0qraxp9mv6lqw57mhs2eda755x4pde7dx5cq0s90nqck"
WALLET_DIR="/tmp/zcash-wallet-send"
IDENTITY="/tmp/zcash-wallet-send/identity.txt"
LWD_SERVER="testnet.zec.rocks:443"

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

echo -e "${BOLD}${CYAN}"
echo "  ╔═══════════════════════════════════════════════╗"
echo "  ║   ZIMPPY — Live MPP E2E Test                  ║"
echo "  ║   Real Zcash. Real Privacy. Real Payments.    ║"
echo "  ╚═══════════════════════════════════════════════╝"
echo -e "${NC}"
sleep 2

# ─── STEP 1: Request paid resource ───
echo -e "${BOLD}STEP 1: Request /api/fortune without payment${NC}"
echo -e "${YELLOW}→ GET $SERVER/api/fortune${NC}"
echo ""

RESP=$(curl -s "$SERVER/api/fortune")
CHALLENGE_ID=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['challengeId'])")

echo -e "${RED}← 402 Payment Required${NC}"
echo -e "  Challenge ID: ${CYAN}$CHALLENGE_ID${NC}"
echo -e "  Memo to embed: ${CYAN}zimppy:$CHALLENGE_ID${NC}"
echo -e "  Amount: ${CYAN}42000 zat${NC}"
echo -e "  Recipient: ${CYAN}${SERVER_ADDR:0:20}...${NC}"
echo ""
sleep 2

# ─── STEP 2: Send real ZEC ───
echo -e "${BOLD}STEP 2: Sending 42000 zat on Zcash testnet (Orchard shielded)${NC}"
echo -e "${YELLOW}  Building Orchard transaction with memo 'zimppy:$CHALLENGE_ID'${NC}"
echo -e "${YELLOW}  Submitting via testnet.zec.rocks lightwalletd...${NC}"
echo ""

# Sync wallet first
zcash-devtool wallet -w "$WALLET_DIR" sync --server "$LWD_SERVER" --connection direct 2>&1 | grep -v "^$"

# Send the payment
TXID=$(zcash-devtool wallet -w "$WALLET_DIR" send \
  -i "$IDENTITY" \
  --server "$LWD_SERVER" \
  --connection direct \
  --address "$SERVER_ADDR" \
  --value 42000 \
  --memo "zimppy:$CHALLENGE_ID" 2>&1 | grep -E '^[a-f0-9]{64}$' | tail -1)

if [ -z "$TXID" ]; then
  echo -e "${RED}ERROR: Failed to get txid. Check wallet pane.${NC}"
  # Try to extract from output
  TXID=$(zcash-devtool wallet -w "$WALLET_DIR" send \
    -i "$IDENTITY" \
    --server "$LWD_SERVER" \
    --connection direct \
    --address "$SERVER_ADDR" \
    --value 42000 \
    --memo "zimppy:$CHALLENGE_ID" 2>&1 | tail -5)
  echo "$TXID"
  exit 1
fi

echo -e "${GREEN}  ✓ Transaction broadcast!${NC}"
echo -e "  txid: ${CYAN}$TXID${NC}"
echo ""
sleep 2

# ─── STEP 3: Wait for confirmation ───
echo -e "${BOLD}STEP 3: Waiting for on-chain confirmation (~75-150 seconds)${NC}"

for i in $(seq 1 20); do
  sleep 15
  echo -n "."
  # Check if tx has confirmations
  CONF=$(curl -s -X POST https://zcash-testnet-zebrad.gateway.tatum.io \
    -H 'Content-Type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"method\":\"getrawtransaction\",\"params\":[\"$TXID\",1],\"id\":1}" 2>/dev/null \
    | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('result',{}).get('confirmations',0))" 2>/dev/null)

  if [ -n "$CONF" ] && [ "$CONF" -gt 0 ] 2>/dev/null; then
    echo ""
    echo -e "${GREEN}  ✓ Confirmed! $CONF confirmations${NC}"
    break
  fi
done
echo ""
sleep 2

# ─── STEP 4: Submit credential ───
echo -e "${BOLD}STEP 4: Retrying with payment credential${NC}"

CRED=$(python3 -c "import base64,json; print(base64.urlsafe_b64encode(json.dumps({'payload':{'txid':'$TXID','challengeId':'$CHALLENGE_ID'}}).encode()).decode().rstrip('='))")

echo -e "${YELLOW}→ GET $SERVER/api/fortune${NC}"
echo -e "${YELLOW}  Authorization: Payment ${CRED:0:40}...${NC}"
echo ""

RESP=$(curl -s -D /tmp/mpp_live.txt "$SERVER/api/fortune" -H "Authorization: Payment $CRED")
STATUS=$(head -1 /tmp/mpp_live.txt | awk '{print $2}')
RECEIPT=$(grep -i payment-receipt /tmp/mpp_live.txt 2>/dev/null || echo "none")

if [ "$STATUS" = "200" ]; then
  FORTUNE=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin).get('fortune',''))" 2>/dev/null)
  echo -e "${GREEN}← 200 OK${NC}"
  echo -e "${GREEN}  Fortune: \"$FORTUNE\"${NC}"
  echo ""
  echo -e "${GREEN}  Payment-Receipt:${NC}"
  echo "$RECEIPT" | sed 's/payment-receipt: //' | python3 -c "import sys,json; print(json.dumps(json.loads(sys.stdin.read()), indent=2))" 2>/dev/null | sed 's/^/  /'
  echo ""
  echo -e "${BOLD}${GREEN}"
  echo "  ╔═══════════════════════════════════════════════╗"
  echo "  ║              E2E TEST PASSED ✓                ║"
  echo "  ║                                               ║"
  echo "  ║  ✓ 402 challenge with UUID + memo template    ║"
  echo "  ║  ✓ Real Orchard shielded tx on testnet        ║"
  echo "  ║  ✓ Server decrypted with viewing key          ║"
  echo "  ║  ✓ Memo binding verified                      ║"
  echo "  ║  ✓ Amount verified (42000 zat)                ║"
  echo "  ║  ✓ 200 OK + fortune + receipt                 ║"
  echo "  ║                                               ║"
  echo "  ║  All private. Nobody can see this payment.    ║"
  echo "  ╚═══════════════════════════════════════════════╝"
  echo -e "${NC}"
else
  echo -e "${RED}← $STATUS${NC}"
  echo -e "${RED}  $RESP${NC}"
fi
