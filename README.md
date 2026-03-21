# zimppy

[![npm](https://img.shields.io/npm/v/zimppy)](https://www.npmjs.com/package/zimppy)
[![crates.io](https://img.shields.io/crates/v/zimppy-core)](https://crates.io/crates/zimppy-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

**Private machine payments on Zcash.** A complete implementation of the [Machine Payments Protocol (MPP)](https://paymentauth.org) using Zcash shielded transactions.

AI agents pay for APIs — but every payment is public. zimppy adds Zcash as a payment rail to MPP, so sender, receiver, amount, and memo are all encrypted on-chain.

### Install

```bash
npm install zimppy        # CLI + wallet
npm install zimppy-ts     # TypeScript SDK
```

```toml
# Cargo.toml (Rust verification engine)
zimppy-core = "0.1"
```

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
# Install
npm install zimppy

# Set up wallet
npx zimppy wallet create

# Start an example server (requires cloning the repo)
cargo run --bin zimppy-rust-server
# → listening on http://0.0.0.0:3180

# Make a paid request
npx zimppy request http://localhost:3180/api/fortune
```

## CLI

The `zimppy` CLI handles payments automatically — discover services, send shielded payments, manage sessions.

```bash
# Create a wallet (generates fresh keys, shows seed phrase)
npx zimppy wallet create

# Multiple named wallets
npx zimppy wallet create work
npx zimppy wallet use work

# Make paid requests
npx zimppy request http://localhost:3180/api/fortune

# Sessions (deposit once, instant repeat requests)
npx zimppy request http://localhost:3180/api/session/fortune
npx zimppy request http://localhost:3180/api/session/fortune  # instant!
npx zimppy session close  # refund unused balance
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
  llm-summarizer/     LLM summarizer server + pay-per-token demo + OpenCode agent
  mcp-server/         MCP tool server with paid AI tools
  ts-server/          TypeScript MPP server
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
