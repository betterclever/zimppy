# zimppy

**Private machine payments on Zcash.** A complete implementation of the [Machine Payments Protocol (MPP)](https://paymentauth.org) using Zcash shielded transactions.

AI agents pay for APIs — but every payment is public. zimppy adds Zcash as a payment rail to MPP, so sender, receiver, amount, and memo are all encrypted on-chain.

## Why?

| | Solana/Tempo MPP | zimppy (Zcash) |
|---|---|---|
| Who paid | Public | Encrypted |
| How much | Public | Encrypted |
| What for | Public | Encrypted memo |
| Verification | Read public ledger | Decrypt with viewing key |

## What's Included

- **Charge** — one-time shielded payment per request (HTTP 402 flow)
- **Sessions** — deposit once, instant bearer requests, close with refund
- **SSE Streaming** — pay-per-word metered content
- **CLI** — `npx zimppy request` with automatic payment handling
- **Dual SDK** — TypeScript + Rust
- **MCP Integration** — paid tools for AI agents
- **Spec-compliant** — HMAC-SHA256 challenges, RFC 9457 errors, `/.well-known/payment` discovery

## How It Works

### Charge (one-time)

```
Agent  →  GET /api/fortune
Server →  402 + challenge (amount, recipient, memo)
Agent  →  sends shielded ZEC with memo "zimppy:{challenge_id}"
Agent  →  GET /api/fortune + Authorization: Payment {txid, challengeId}
Server →  decrypts with Orchard IVK, verifies amount + memo
Server →  200 OK + Payment-Receipt
```

### Session (prepaid balance)

```
Agent  →  deposits 100,000 zat (1 on-chain tx)
Agent  →  open session → gets session_id + bearer token
Agent  →  bearer request → instant, no blockchain (sub-ms)
Agent  →  bearer request → instant again...
Agent  →  close → unused balance refunded (1 on-chain tx)
```

### SSE Streaming (pay-per-token)

```
Agent  →  opens session with deposit
Agent  →  GET /api/stream/fortune (SSE)
Server →  streams word-by-word, deducting per word
Agent  →  close → refund of unused balance
```

## Quick Start

```bash
# Build
cargo build --workspace
npm install

# Start the MPP server
PRICE_ZAT=10000 cargo run --bin zimppy-rust-server
# → listening on http://0.0.0.0:3180

# Try it
curl http://localhost:3180/api/fortune
# → 402 Payment Required

curl http://localhost:3180/.well-known/payment
# → service discovery
```

## CLI

The `zimppy` CLI handles payments automatically — discover services, send shielded payments, manage sessions.

```bash
# Setup wallet
npx zimppy wallet login
npx zimppy wallet whoami

# One-time paid request
npx zimppy request http://localhost:3180/api/fortune

# Session (deposits 10x, subsequent requests are instant)
npx zimppy request http://localhost:3180/api/session/fortune
npx zimppy request http://localhost:3180/api/session/fortune  # instant!
npx zimppy session close  # refund unused balance

# Custom deposit amount
npx zimppy request --deposit 500000 http://localhost:3180/api/session/fortune
```

## Demos

All demos use real Zcash testnet transactions. Each launches a tmux session with server (left) and client (right).

```bash
# Fortune Teller — charge, session, and streaming demos
bash examples/fortune-teller/demos/charge/run.sh
bash examples/fortune-teller/demos/session/run.sh
bash examples/fortune-teller/demos/stream/run.sh

# LLM Summarizer — AI document summarization with pay-per-token
bash examples/llm-summarizer/demos/run.sh
```

## Architecture

```
crates/
  zimppy-core/        Zcash verification engine (Orchard decryption, replay protection)
  zimppy-napi/        NAPI-RS bindings (native Rust from Node.js)
  zimppy-rs/          Rust SDK (ChargeMethod, SessionMethod, PaymentProvider, SSE)
  zimppy-wallet/      Native Zcash wallet (zingolib)
packages/
  zimppy-ts/          TypeScript SDK (mppx-native, charge, session, SSE)
  zimppy-cli/         CLI with auto-pay and session management
examples/
  fortune-teller/     Fortune teller MPP server + charge/session/stream demos
  llm-summarizer/     AI summarizer server + pay-per-token demo
  mcp-server/         MCP tool server with paid AI tools
  ts-server/          TypeScript MPP server
  opencode-agent/     AI agent demo (OpenCode + VibeProxy)
config/
  server-wallet.example.json  Server wallet config template
```

## Spec Compliance

| MPP Feature | Status |
|---|---|
| HTTP 402 + `WWW-Authenticate: Payment` | Done |
| HMAC-SHA256 challenge IDs | Done |
| `Authorization: Payment` credentials | Done |
| `Payment-Receipt` header | Done |
| RFC 9457 Problem Details | Done |
| Charge intent | Done |
| Session intent (open/bearer/topUp/close) | Done |
| SSE streamed payments | Done |
| `/.well-known/payment` discovery | Done |
| MCP transport | Done |
| Replay protection | Done |
| Challenge-bound memo verification | Done |

## Tested on Zcash Testnet

All features verified with real shielded transactions — Orchard decryption with viewing key, memo binding, session lifecycle with on-chain refunds.

## License

MIT
