# zimppy

**Private machine payments on Zcash.** A complete implementation of the [Machine Payments Protocol (MPP)](https://paymentauth.org) for privacy-preserving blockchains.

AI agents pay for APIs — but every payment is public. Anyone can see which agent paid for what, how much, and when. zimppy adds Zcash as a payment rail to MPP, enabling fully shielded machine-to-machine payments where the sender, receiver, amount, and memo are all encrypted on-chain.

## Features

- **Charge** — one-time payment per request with on-chain Orchard verification
- **Sessions** — prepaid balance: deposit once, instant bearer requests, close with refund
- **SSE Streaming** — pay-per-token metered content delivery
- **MCP Integration** — paid tools for AI agents via Model Context Protocol
- **HMAC-SHA256 Challenge IDs** — spec-compliant challenge binding
- **RFC 9457 Problem Details** — standard error responses
- **Discovery** — `/.well-known/payment` endpoint
- **Privacy** — Orchard shielded transactions with encrypted memo binding
- **Dual SDK** — TypeScript (mppx-native) + Rust (mpp-rs ChargeMethod + PaymentProvider)
- **NAPI Bindings** — Rust verification called natively from Node.js (zero HTTP overhead)

## Privacy vs Public Chains

| | Solana/Tempo MPP | zimppy (Zcash) |
|---|---|---|
| Who paid | Public | Hidden |
| How much | Public | Hidden |
| What for | Public | Encrypted memo |
| Verification | Read public ledger | Decrypt with viewing key |

## How It Works

### Charge (one-time payment)

```
Agent → GET /api/fortune
Server → 402 + WWW-Authenticate: Payment method="zcash", challenge_id, memo template
Agent → sends ZEC on-chain with memo "zimppy:{challenge_id}"
Agent → GET /api/fortune + Authorization: Payment {txid, challengeId}
Server → decrypts Orchard tx with IVK, verifies amount + memo
Server → 200 OK + fortune + Payment-Receipt
```

### Session (prepaid balance)

```
Agent → deposits 100,000 zat (1 on-chain tx)
Agent → open: server creates session, bearer = sha256(deposit_txid)
Agent → bearer: instant requests, server deducts balance (0 on-chain txs!)
Agent → bearer: another request... (sub-millisecond, no blockchain)
Agent → close: server refunds unused balance (1 on-chain tx)
```

### SSE Streaming (pay-per-token)

```
Agent → opens session with deposit
Agent → GET /api/stream/fortune (SSE)
Server → streams word-by-word, deducting 1000 zat per word
Server → emits payment-need-voucher if balance runs out
Agent → sends topUp to continue
Server → emits payment-receipt at end
Agent → closes session, gets refund
```

## Architecture

```
                    zimppy-core (Rust)
                    Zcash verification engine
                     /                \
                NAPI bindings      Rust crate
                (in-process)       (zero overhead)
                   /                    \
            zimppy-ts               zimppy-rs
            TypeScript SDK          Rust SDK
            mppx-native             mpp-rs ChargeMethod
                |                       |
         MCP tool server          Rust MPP server
         (paid AI tools)          (charge + session + stream)
```

## Quick Start

```bash
# build
cargo build --workspace
npm install

# start the MPP server
cargo run --bin zimppy-rust-server
# → http://0.0.0.0:3180

# request a paid resource
curl http://localhost:3180/api/fortune
# → 402 Payment Required
# → WWW-Authenticate: Payment method="zcash", intent="charge", ...

# discovery
curl http://localhost:3180/.well-known/payment
```

## Live Demos

All demos use real Zcash testnet transactions — real money, real privacy.

```bash
# Charge flow: 402 → send ZEC → verify → 200
bash scripts/live-e2e.sh

# Session flow: deposit → 5 instant requests → close with refund
bash scripts/demo-session.sh

# SSE streaming: deposit → pay-per-word fortune → refund
bash scripts/demo-stream.sh
```

## Packages

### zimppy-core (Rust)

Zcash verification engine — transparent + Orchard shielded.

```rust
use zimppy_core::{ZebradRpc, verify_transparent, ConsumedTxids};
use zimppy_core::shielded::verify_shielded;

// Shielded verification with Orchard IVK + memo binding
let result = verify_shielded(&rpc, &ShieldedVerifyRequest {
    txid: "abc123...".into(),
    ivk_bytes_hex: "803af23f...".into(),
    expected_challenge_id: "challenge-001".into(),
    expected_amount_zat: 42_000,
}, &consumed).await?;
// result.verified, result.memo_matched, result.observed_amount_zat
```

### zimppy-ts (TypeScript, mppx-native)

```typescript
import { zcashServer, zcashClient, zcashSessionServer } from 'zimppy-ts'

// Server — verify payments via NAPI (native Rust in-process)
const server = zcashServer({ orchardIvk: '803af23f...', rpcEndpoint: '...' })

// Client — auto-pay with createPayment callback
const client = zcashClient({
  createPayment: async ({ challenge }) => {
    const txid = await sendZec(challenge.recipient, challenge.amount, challenge.memo)
    return { txid }
  }
})

// Sessions
const sessions = zcashSessionServer({ orchardIvk, crypto, store, recipient, network })
```

### zimppy-rs (Rust, mpp-rs native)

```rust
use zimppy_rs::{ZcashChargeMethod, ZcashSessionMethod, ZcashPaymentProvider};

// Server — ChargeMethod trait (mpp-rs compatible)
let charge = ZcashChargeMethod::new(rpc_endpoint, recipient, orchard_ivk);
let outcome = charge.verify_payment(txid, challenge_id, amount).await?;

// Sessions
let session = ZcashSessionMethod::new(rpc_endpoint, orchard_ivk);
let result = session.verify_session(&payload, charge_amount).await?;

// Client — PaymentProvider trait (mpp-rs compatible)
let provider = ZcashPaymentProvider::new(wallet_dir, identity, lwd_server, rpc);
```

## Server Wallet Setup

```bash
# Generate a server wallet with Orchard address + IVK
cargo run --bin zimppy-derive-ivk --features keygen -- "your 24 word mnemonic"
# → Address: utest1...
# → Orchard IVK: 803af23f...

# Save to config
cat config/server-wallet.json
# { "address": "utest1...", "orchardIvk": "803af23f...", "network": "testnet" }
```

## MCP Server

Paid tools for AI agents via Model Context Protocol:

```bash
# Start (reads config/server-wallet.json)
npx tsx apps/mcp-server/src/server.ts
```

Tools: `get_weather` (42,000 zat), `get_zcash_info` (10,000 zat), `ping` (free).

Payment via `_meta["org.paymentauth/credential"]` per [draft-payment-transport-mcp-00](https://paymentauth.org/draft-payment-transport-mcp-00.html).

## Spec Compliance

| MPP Spec Feature | Status |
|---|---|
| HTTP 402 + `WWW-Authenticate: Payment` | ✓ |
| HMAC-SHA256 challenge IDs | ✓ |
| `Authorization: Payment` credentials | ✓ |
| `Payment-Receipt` header | ✓ |
| RFC 9457 Problem Details | ✓ |
| Charge intent (one-time) | ✓ |
| Session intent (prepaid) | ✓ |
| SSE streamed payments | ✓ |
| Discovery (`/.well-known/payment`) | ✓ |
| MCP transport (`-32042` / `-32043`) | ✓ |
| mppx `Method.from` / `toServer` / `toClient` | ✓ |
| mpp-rs `ChargeMethod` trait | ✓ |
| mpp-rs `PaymentProvider` trait | ✓ |
| Replay protection | ✓ |
| Challenge-bound memo verification | ✓ |

## Proven on Zcash Testnet

All features tested with real Zcash transactions:

- **Charge**: txid `0cc4036a...` — 42,000 zat, memo verified, fortune served
- **Session**: deposit → 5 instant bearer requests → close with 40,000 zat refund
- **Streaming**: 19 words streamed at 1,000 zat/word → 26,000 zat refunded
- **Shielded**: Orchard decryption with viewing key — nobody else can see the payment

## Project Structure

```
crates/
  zimppy-core/      Zcash verification engine (RPC, transparent, shielded, replay)
  zimppy-napi/      NAPI-RS bindings for Node.js
  zimppy-rs/        Rust SDK (ChargeMethod, SessionMethod, PaymentProvider, SSE)
packages/
  zimppy-ts/        TypeScript SDK (mppx-native, charge, session, SSE)
apps/
  rust-server/      Rust MPP server (charge + session + stream endpoints)
  ts-server/        TypeScript HTTP payment server
  mcp-server/       MCP tool server with paid tools
  demo/             E2E demo scripts (autopay, mcp-pay, block-progress)
scripts/
  live-e2e.sh       Charge flow E2E test
  demo-session.sh   Session flow tmux demo
  demo-stream.sh    SSE streaming tmux demo
config/
  server-wallet.json  Server Orchard address + IVK
research/
  01-mpp-specification.md
  02-solana-mpp-implementation.md
  03-zcash-capabilities.md
  04-zcash-tooling.md
  05-feasibility-analysis.md
  06-session-implementation-guide.md
```

## License

MIT
