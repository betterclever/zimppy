#!/usr/bin/env bash
# Live MPP E2E test — runs the TypeScript mppx auto-pay demo in the client pane
set -euo pipefail

cd "$(dirname "$0")/.."

export SERVER_URL="${SERVER_URL:-http://127.0.0.1:3180}"
export ZCASH_WALLET_DIR="${ZCASH_WALLET_DIR:-/tmp/zcash-wallet-send}"
export ZCASH_IDENTITY_FILE="${ZCASH_IDENTITY_FILE:-/tmp/zcash-wallet-send/identity.txt}"
export ZCASH_LWD_SERVER="${ZCASH_LWD_SERVER:-testnet.zec.rocks:443}"
export ZCASH_RPC_ENDPOINT="${ZCASH_RPC_ENDPOINT:-https://zcash-testnet-zebrad.gateway.tatum.io}"
export ZCASH_CONFIRMATION_TIMEOUT_MS="${ZCASH_CONFIRMATION_TIMEOUT_MS:-300000}"
export ZCASH_CONFIRMATION_POLL_MS="${ZCASH_CONFIRMATION_POLL_MS:-15000}"

echo "=== Running TypeScript mppx auto-pay demo ==="
echo "Server: $SERVER_URL"
echo "Wallet: $ZCASH_WALLET_DIR"
echo "Lightwalletd: $ZCASH_LWD_SERVER"
echo "RPC: $ZCASH_RPC_ENDPOINT"
echo

npx tsx apps/demo/pay.ts
