# zimppy

[![crates.io](https://img.shields.io/crates/v/zimppy-core)](https://crates.io/crates/zimppy-core)
[![npm](https://img.shields.io/npm/v/zimppy)](https://www.npmjs.com/package/zimppy)
[![npm](https://img.shields.io/npm/v/zimppy-ts?label=zimppy-ts)](https://www.npmjs.com/package/zimppy-ts)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

The privacy stack for [MPP](https://mpp.dev). Zcash shielded transactions today, more privacy rails coming.

[zimppy.xyz](https://zimppy.xyz)

## Install

```bash
npm install zimppy          # CLI + wallet
npm install zimppy-ts       # TypeScript SDK
```

```toml
[dependencies]
zimppy-core = "0.3"         # Rust verification engine
zimppy-rs = "0.3"           # Rust SDK (charge, session, axum)
```

## What's included

- **Charge** — one shielded payment per request (HTTP 402)
- **Sessions** — deposit once, instant bearer requests, close with refund
- **Streaming** — pay-per-token metered content over SSE
- **CLI** — `npx zimppy request` with automatic payment handling
- **Dual SDK** — TypeScript and Rust
- **Spec-compliant** — HMAC-SHA256 challenges, RFC 9457 errors, `/.well-known/payment` discovery

## Why

| | Public chains | zimppy |
|---|---|---|
| Sender | Visible | Encrypted |
| Receiver | Visible | Encrypted |
| Amount | Visible | Encrypted |
| Memo | Visible | Encrypted |

## How it works

### Charge

Single shielded payment per request. Standard HTTP 402 flow.

```
Agent  ->  GET /api/fortune
Server ->  402 + challenge (amount, recipient, memo)
Agent  ->  shielded ZEC with memo "zimppy:{challenge_id}"
Agent  ->  GET /api/fortune + Authorization: Payment {txid}
Server ->  decrypt with Orchard IVK, verify amount + memo
Server ->  200 OK + Payment-Receipt
```

### Session

Deposit once on-chain, then instant bearer requests with no chain interaction.

```
Agent  ->  deposit 100,000 zat           (on-chain)
Agent  ->  open session                  (bearer token)
Agent  ->  request, request, request     (instant)
Agent  ->  close session                 (refund unused balance)
```

### Streaming

Pay-per-token over SSE. Server deducts from session balance per word streamed.

```
Agent  ->  open session with deposit
Agent  ->  GET /api/stream (SSE)
Server ->  stream word by word, deducting per token
Agent  ->  close session, refund remaining
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

// 402 -> pay -> retry handled automatically
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
npx zimppy wallet whoami              # address, balance, network
npx zimppy request <url>              # auto 402 -> pay -> retry
npx zimppy wallet send <addr> 42000   # shielded transfer
npx zimppy wallet use work            # switch wallet identity
```

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
