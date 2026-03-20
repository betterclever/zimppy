# Zimppy — Private Machine Payments on Zcash

Zimppy is a curl-compatible CLI for making HTTP requests with automatic private Zcash payments. When an API returns 402 Payment Required, zimppy pays with shielded ZEC and retries — transparently.

## Setup

1. **Wallet must be configured** at the path in `$ZCASH_WALLET_DIR` (default: `/tmp/zcash-wallet-send`) with identity file at `$ZCASH_IDENTITY_FILE`.

2. **Verify wallet is ready:**
```bash
npx tsx apps/demo/zimppy-cli.ts wallet whoami
```

3. **Check balance:**
```bash
npx tsx apps/demo/zimppy-cli.ts wallet balance
```

If balance is 0, fund the wallet via a Zcash testnet faucet or another wallet.

## Making Paid Requests

Use `zimppy request` exactly like `curl` — it handles 402 responses automatically:

```bash
npx tsx apps/demo/zimppy-cli.ts request http://localhost:3180/api/fortune
```

This will:
- Send a GET request
- If 402 returned: parse challenge, send shielded ZEC, wait for confirmation, retry
- Print the result to stdout

### POST with JSON body:
```bash
npx tsx apps/demo/zimppy-cli.ts request -X POST --json '{"city":"Tokyo"}' http://localhost:3180/api/weather
```

## Service Discovery

Check what payment methods a server supports:
```bash
npx tsx apps/demo/zimppy-cli.ts discover http://localhost:3180
```

## Important Rules

1. **Always use full command path** — `npx tsx apps/demo/zimppy-cli.ts` from the project root
2. **Discover before requesting** — check `/.well-known/payment` to see pricing and methods
3. **Wallet must be funded** — check balance before making paid requests
4. **Results go to stdout, logs to stderr** — pipe-friendly for automation
5. **Payments are private** — sender, receiver, amount, and memo are all encrypted on-chain
6. **Confirmations take ~75 seconds** — shielded Zcash transactions need 1 block confirmation

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `ZCASH_WALLET_DIR` | `/tmp/zcash-wallet-send` | Path to zcash-devtool wallet |
| `ZCASH_IDENTITY_FILE` | `$WALLET_DIR/identity.txt` | Age identity file for signing |
| `ZCASH_LWD_SERVER` | `testnet.zec.rocks:443` | Lightwalletd server |
| `ZCASH_RPC_ENDPOINT` | Tatum public testnet | Zebrad RPC for confirmations |

## Example Session

```
$ npx tsx apps/demo/zimppy-cli.ts discover http://localhost:3180
{
  "methods": ["zcash"],
  "intents": ["charge"],
  "currency": "ZEC",
  "defaultAmount": "42000",
  "recipient": "utest1..."
}

$ npx tsx apps/demo/zimppy-cli.ts request http://localhost:3180/api/fortune
→ GET http://localhost:3180/api/fortune
← 402 Payment Required
  Amount: 42000 zat
  To: utest18332muyrx6kw4z...
  Memo: zimppy:e1274dab...
→ Paying with Zcash...
  Syncing wallet...
  Sending 42000 zat...
  Broadcast: txid=abc123...
  Waiting for confirmation...
  Confirmed! 1 confirmation
→ Retrying with credential...
← 200
{"fortune":"Privacy is not about having something to hide."}
  Receipt: method=zcash, reference=abc123...
```

All payments are fully private — nobody can see this transaction on-chain except the server.
