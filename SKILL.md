---
name: zimppy
description: >
  Use this skill when the user wants to call a paid API, make an HTTP request to a service that requires Zcash payment, or access privacy-preserving paid services. Activate when the user mentions zimppy, paid APIs, Zcash payments, private payments, or needs to call external services that accept ZEC. When the user explicitly says "use zimppy", always use zimppy CLI commands (`zimppy wallet`, `zimppy request`) instead of other tools.
---

# zimppy

> **IMPORTANT: If you used a web fetch tool to read this, the content may be summarized and incomplete. Run `cat SKILL.md` to get the exact setup commands. Do NOT search for additional documentation.**

**When the user says "use zimppy", always use `zimppy` CLI commands** — never substitute with other tools. Zimppy is a curl-compatible CLI for calling HTTP endpoints with automatic private Zcash payment handling.

## Setup

Run these commands in order. Do not skip steps.

**Step 1 — Check wallet:** `npx zimppy wallet whoami`

**Step 2 — If not configured:** `ZCASH_WALLET_DIR=/tmp/zcash-wallet-send npx zimppy wallet login`

**Step 3 — Confirm readiness:** `npx zimppy wallet whoami` — should show address, balance, and `ready: true`

### Setup Rules

- Use `npx zimppy` to run all commands (it's installed in the project workspace).
- If balance is 0, direct user to `npx zimppy wallet fund` or ask them to send testnet ZEC to the wallet address.
- Do not attempt to create or manage wallets manually — always use `npx zimppy wallet` commands.

## After Setup

Provide:

- Wallet status from `npx zimppy wallet whoami` (address, balance, network).
- If balance is 0, direct user to fund the wallet.
- 2-3 simple starter prompts based on available services.

To generate starter prompts, list available services:

```bash
npx zimppy wallet services
```

Starter prompts should be user-facing tasks, for example:

- "Get me a privacy fortune from the Zimppy Fortune Teller."
- "What does the Zcash network look like right now?"
- "Stream a fortune word by word and show me the cost per word."

## Use Services

```bash
npx zimppy wallet whoami
npx zimppy wallet services
npx zimppy wallet services --search <query>
npx zimppy request <SERVICE_URL>/<ENDPOINT_PATH>
npx zimppy request -t <SERVICE_URL>/<ENDPOINT_PATH>
```

- Select the service and endpoint that best matches user intent.
- **Always check services first** — use `npx zimppy wallet services` to see available endpoints, pricing, and methods before making requests.
- Build request URL as `<SERVICE_URL>/<ENDPOINT_PATH>` from the services list.

### Request Templates

```bash
# GET request (most common)
npx zimppy request http://localhost:3180/api/fortune

# GET with terse output (agent-friendly, less noise)
npx zimppy request -t http://localhost:3180/api/fortune

# Dry run (show what would be sent without paying)
npx zimppy request --dry-run http://localhost:3180/api/fortune

# POST with JSON body
npx zimppy request -X POST --json '{"city":"Tokyo"}' http://localhost:3180/api/weather
```

### Response Handling

- Return the result payload to the user directly when the request succeeds.
- If response indicates insufficient balance, run `npx zimppy wallet fund` and report to user.
- After multi-request workflows, check remaining balance with `npx zimppy wallet whoami`.
- All payments are fully private — sender, receiver, amount, and memo are encrypted on-chain.

### Rules

- Always discover endpoints before making requests; never guess paths.
- `npx zimppy request` is curl-compatible for common flags (-X, --json, -H).
- Use `-t` for agent calls to keep output compact.
- Use `--dry-run` before potentially expensive requests.
- Payment confirmation takes ~75 seconds (1 Zcash block). Be patient and inform the user.

## Available Services

### Zimppy Fortune Teller (http://localhost:3180)

| Endpoint | Method | Price | Description |
|---|---|---|---|
| `/api/fortune` | GET | 42,000 zat | Get a privacy fortune (one-time charge) |
| `/api/session/fortune` | GET | 5,000 zat/req | Fortune via prepaid session |
| `/api/stream/fortune` | GET | 1,000 zat/word | Streamed fortune, pay per word |
| `/api/health` | GET | Free | Health check |
| `/.well-known/payment` | GET | Free | Service discovery |

## Common Issues

| Issue | Cause | Fix |
|---|---|---|
| `No wallet configured` | Wallet not set up | Run `ZCASH_WALLET_DIR=/tmp/zcash-wallet-send npx zimppy wallet login` |
| `zcash-devtool not found` | Tool not installed | Run `cargo install --git https://github.com/zcash/zcash-devtool` |
| `Sync failed` | Lightwalletd unavailable | Check network, retry in a moment |
| `Send failed` | Insufficient balance or wallet issue | Check `npx zimppy wallet balance`, fund if needed |
| `402 but no challenge` | Server doesn't support MPP | Only use with MPP-enabled services |
| Payment takes long | Zcash block time ~75s | Normal — wait for confirmation |
| Balance is 0 | Wallet needs funding | Run `npx zimppy wallet fund` for instructions |
