# zimppy

[![crates.io](https://img.shields.io/crates/v/zimppy-core)](https://crates.io/crates/zimppy-core)
[![npm](https://img.shields.io/npm/v/zimppy)](https://www.npmjs.com/package/zimppy)
[![npm](https://img.shields.io/npm/v/zimppy-ts?label=zimppy-ts)](https://www.npmjs.com/package/zimppy-ts)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

**The most capable Zcash CLI wallet for agents and humans.**

Zimppy is the [MPP](https://mpp.dev) payment method for Zcash — shielded and transparent. Deposit once on-chain, then make unlimited instant bearer requests with no per-request chain interaction. Shielded payments encrypt sender, receiver, amount, and memo. Transparent payments use per-challenge T-addresses for replay prevention.

[zimppy.xyz](https://zimppy.xyz)

## Install

```bash
npm install zimppy          # CLI + wallet
npm install zimppy-ts       # TypeScript SDK
```

```toml
[dependencies]
zimppy-core = "0.5"         # Rust verification engine
zimppy-rs = "0.5"           # Rust SDK (charge, session, axum)
```

## Why shielded payments for agents

| | Public chains (USDC, ETH) | zimppy shielded | zimppy transparent |
|---|---|---|---|
| Sender | Visible | Encrypted | Visible |
| Receiver | Visible | Encrypted | Per-challenge (unlinkable) |
| Amount | Visible | Encrypted | Visible |
| Memo | Visible | Encrypted | N/A |
| Replay protection | None | Memo binding | Per-challenge T-address |
| Service usage pattern | Linkable | Private | Unlinkable (fresh addr) |

For AI agents handling sensitive workflows — legal research, medical queries, financial analysis, competitive intelligence — every public payment is a metadata leak. Zimppy is the only MPP payment method that is private by default.

## The latency answer: sessions

"But Zcash has 75-second block times."

Sessions solve this. The on-chain wait happens once at deposit. Every subsequent request is instant.

```
Agent  →  deposit 100,000 zat          (one on-chain tx, ~75s)
Agent  →  open session                 (bearer token issued)
Agent  →  request → response           (0ms — no chain interaction)
Agent  →  request → response           (0ms — no chain interaction)
Agent  →  request → response           (0ms — no chain interaction)
           ... hundreds of requests ...
Agent  →  close session                (refund unused balance)
```

Pay once, call instantly, get back the change. Per-request latency is zero.

## How it works

### Session (recommended)

```
Agent  →  deposit 100,000 zat           (on-chain, ~75s one-time)
Agent  →  open session                  (bearer token issued)
Agent  →  GET /api/query + bearer       (instant, balance deducted)
Agent  →  GET /api/query + bearer       (instant, balance deducted)
Agent  →  close session                 (refund unused balance on-chain)
```

### Streaming

Pay per token over SSE. Server deducts from session balance per word streamed.

```
Agent  →  open session with deposit
Agent  →  GET /api/stream (SSE)
Server →  stream word by word, deducting per token
Agent  →  close session, refund remaining
```

### Charge

Single shielded payment per request. Use when the request is infrequent or high-value enough that a one-time ~75s confirmation is acceptable.

```
Agent  →  GET /api/resource
Server →  402 + challenge (amount, recipient, memo)
Agent  →  shielded ZEC with memo "zimppy:{challenge_id}"
Agent  →  GET /api/resource + Authorization: Payment {txid}
Server →  decrypt with Orchard IVK, verify amount + memo
Server →  200 OK + Payment-Receipt
```

## Server

**TypeScript**
```ts
import { Mppx } from 'mppx/server'
import { zcash } from 'zimppy-ts/server'

const mppx = Mppx.create({
  methods: [await zcash({ wallet: 'server' })],
  realm: 'my-api',
  secretKey: process.env.MPP_SECRET_KEY,
})

const result = await mppx.charge({
  amount: '42000',
  currency: 'zec',
})(request)

if (result.status === 402) return result.challenge
return result.withReceipt(Response.json({ data }))
```

**TypeScript (transparent)**
```ts
import { Mppx } from 'mppx/server'
import { zcashTransparent } from 'zimppy-ts/server'

const mppx = Mppx.create({
  methods: [await zcashTransparent({ wallet: 'server' })],
  // per-challenge T-address generated automatically (replay-safe)
})
```

**Rust (axum)**
```rust
use mpp::server::axum::*;
use zimppy_rs::ZcashChallenger;

struct Price;
impl ChargeConfig for Price {
    fn amount() -> &'static str { "42000" }
}

async fn handler(charge: MppCharge<Price>) -> WithReceipt<Json<Value>> {
    WithReceipt { receipt: charge.receipt, body: Json(data) }
}
```

## Client

**TypeScript**
```ts
import { Mppx } from 'mppx/client'
import { zcash } from 'zimppy-ts/client'

const mppx = Mppx.create({ methods: [zcash({ wallet: 'default' })] })

// Session opened automatically, 402 handled transparently
const res = await mppx.fetch('https://api.example.com/resource')
```

**Rust**
```rust
use mpp::client::Fetch;
use zimppy_rs::ZcashPaymentProvider;

let provider = ZcashPaymentProvider::new(wallet_config, &rpc);

let resp = client
    .get("https://api.example.com/resource")
    .send_with_payment(&provider)
    .await?;
```

## CLI

```bash
npx zimppy wallet create              # generate keys, show seed phrase
npx zimppy wallet whoami              # address (UA + T-addr), balance, network
npx zimppy wallet balance --all       # per-account balance breakdown
npx zimppy wallet send <addr> 42000   # shielded or transparent transfer
npx zimppy wallet transfer 0 1 50000  # cross-account send
npx zimppy wallet shield              # move transparent funds to Orchard
npx zimppy wallet use work            # switch wallet identity
npx zimppy request <url>              # auto 402 -> pay -> retry
```

## What's included

- **Sessions** — deposit once, instant bearer requests, refund on close
- **Streaming** — pay-per-token metered content over SSE
- **Charge** — shielded or transparent payment per request (HTTP 402)
- **Transparent payments** — T-addresses, per-challenge replay prevention, shield command
- **Multi-account** — ZIP-32 account rotation, cross-account transfers, per-account balances
- **CLI wallet** — send, shield, transfer, balance --all, whoami, auto-pay
- **Dual SDK** — TypeScript and Rust
- **Spec-compliant** — HMAC-SHA256 challenges, RFC 9457 errors, `/.well-known/payment` discovery

## Architecture

```
crates/
  zimppy-core/       Zcash verification engine (Orchard decryption, replay protection)
  zimppy-wallet/     Native Zcash wallet (zingolib)
  zimppy-rs/         Rust SDK (ChargeMethod, SessionMethod, PaymentProvider, axum extractors)
  zimppy-napi/       Node.js native bindings (NAPI-RS)
packages/
  zimppy-ts/         TypeScript SDK (charge, session, SSE)
  zimppy-cli/        CLI with auto-pay and session management
examples/
  fortune-teller/    Charge, session, and streaming demos (Rust server + client)
  llm-summarizer/    Pay-per-token LLM demo
  mcp-server/        MCP tool server with paid AI tools
  ts-server/         TypeScript MPP server
```

## Packages

| Package | Description | Registry |
|---|---|---|
| [zimppy-core](crates/zimppy-core) | Zcash verification engine | [crates.io](https://crates.io/crates/zimppy-core) |
| [zimppy-wallet](crates/zimppy-wallet) | Native Zcash wallet (zingolib) | — |
| [zimppy-rs](crates/zimppy-rs) | Rust SDK: charge, session, axum extractors | — |
| [zimppy-ts](packages/zimppy-ts) | TypeScript SDK | [npm](https://npmjs.com/package/zimppy-ts) |
| [zimppy](packages/zimppy-cli) | CLI with auto-pay | [npm](https://npmjs.com/package/zimppy) |
| [@zimppy/core-napi](crates/zimppy-napi) | Node.js native bindings (darwin-arm64, linux-x64) | [npm](https://npmjs.com/package/@zimppy/core-napi) |

## License

MIT
