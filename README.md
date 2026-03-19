# zimppy

**Private machine payments on Zcash.** The first implementation of the [Machine Payments Protocol](https://paymentauth.org) for a privacy-preserving blockchain.

AI agents need to pay for APIs. Today, every payment is public — anyone can see which agent paid for what, how much, and when. zimppy fixes this by adding Zcash as a payment rail to MPP, enabling fully shielded machine-to-machine payments.

## Why Zcash for MPP?

MPP (Machine Payments Protocol) by Stripe/Tempo uses HTTP 402 to let agents pay for resources inline. Existing implementations (Solana, Tempo) work great but every payment is on a public ledger.

Zcash shielded transactions hide the sender, receiver, amount, and memo. With zimppy, **nobody can see that your agent paid for a service** — not even which service, or how much.

| | Solana MPP | zimppy |
|---|---|---|
| Who paid | Public | Hidden |
| How much | Public | Hidden |
| What for | Public | Encrypted in 512-byte memo |
| Verification | Read public chain | Decrypt with viewing key |
| Fees | ~$0.00025 | ~$0.001 |

## How it works

```
1. Agent requests a paid API
2. Server returns 402 + challenge: "send 42000 zat to ztestsapling1..."
3. Agent sends ZEC (shielded, with challenge ID in encrypted memo)
4. Agent retries with txid as proof
5. Server decrypts the tx with its viewing key, verifies amount + memo
6. Server returns the resource + receipt
```

The server holds only an *incoming viewing key* — it can see payments sent to it, but cannot spend the funds. The spending key stays offline.

## Architecture

```
                      zimppy-core (Rust)
                      Zcash verification engine
                       /                \
                  NAPI bindings      Rust crate import
                  (in-process)       (zero overhead)
                     /                    \
              zimppy-ts               zimppy-rs
              TypeScript SDK          Rust SDK
              for Node.js             for axum/tower
                  |                       |
           MCP tool server         Rust API server
           (paid AI tools)         (402 payment flow)
```

**zimppy-core** is the shared Rust engine. It talks to Zcash nodes via JSON-RPC, parses transactions, decrypts shielded outputs, and enforces replay protection.

**zimppy-ts** is the TypeScript SDK. It calls zimppy-core natively through NAPI-RS (no HTTP hop, no serialization — Rust runs in-process inside Node.js).

**zimppy-rs** is the Rust SDK. It imports zimppy-core as a crate and implements the `ChargeMethod` pattern for direct integration with [mpp-rs](https://github.com/tempoxyz/mpp-rs).

## Quick start

```bash
# build everything
cargo build --workspace
npm install

# build the NAPI native module (needs Node 22+)
cd crates/zimppy-napi && npx napi build --release --platform

# start the verification backend
cargo run --bin zimppy-core-server
# -> listening on http://127.0.0.1:3181

# start the example server
cargo run --bin zimppy-rust-server
# -> listening on http://127.0.0.1:3180

# hit the paid endpoint
curl -i http://localhost:3180/api/fortune
# -> 402 Payment Required
# -> WWW-Authenticate: Payment method="zcash", intent="charge", ...
```

## Generate a testnet wallet

```bash
cargo run --bin zimppy-keygen --features keygen
```

Outputs a Unified Address (for receiving) and a Sapling IVK (for server-side verification). Fund it at [testnet.zecfaucet.com](https://testnet.zecfaucet.com).

## Packages

### zimppy-core

Zcash verification engine. Transparent (check outputs) + shielded (decrypt with viewing key).

```rust
let rpc = ZebradRpc::new("https://zcash-testnet-zebrad.gateway.tatum.io");
let consumed = ConsumedTxids::new();

let result = verify_transparent(&rpc, &TransparentVerifyRequest {
    txid: "abc123...".into(),
    output_index: 0,
    expected_address: "tmXYZ...".into(),
    expected_amount_zat: 42_000,
}, &consumed).await?;
```

Shielded verification (behind `--features shielded`):

```rust
let result = verify_shielded(&rpc, &ShieldedVerifyRequest {
    txid: "def456...".into(),
    ivk_hex: "db5f6a41...".into(),
    expected_challenge_id: "challenge-123".into(),
    expected_amount_zat: 42_000,
}, &consumed).await?;
```

### zimppy-ts

```typescript
import { ZcashChargeServer } from 'zimppy-ts'

const server = new ZcashChargeServer({
  recipient: 'tmXYZ...',
  network: 'testnet',
})

// 402 challenge
const challenge = server.createChallenge('42000')
response.setHeader('WWW-Authenticate', server.formatWwwAuthenticate(challenge))

// verify payment
const credential = server.parseCredential(req.headers.authorization)
const receipt = await server.verify(credential, '42000')
```

Uses NAPI natively when `@zimppy/core-napi` is available, falls back to HTTP.

### zimppy-rs

```rust
let method = ZcashChargeMethod::new(rpc_endpoint, recipient);
let outcome = method.verify_payment(txid, 0, 42_000).await?;
```

### NAPI

```typescript
import { ZimppyCore } from '@zimppy/core-napi'

const core = new ZimppyCore('https://zcash-testnet-zebrad.gateway.tatum.io')
const result = await core.verifyTransparent(txid, 0, address, '42000')
// Rust verification running in-process. No HTTP. No serialization.
```

## MCP server

The `apps/mcp-server` exposes paid tools for AI agents:

- `get_weather` — 42,000 zat
- `get_zcash_info` — 10,000 zat
- `ping` — free

Payment flows through `_meta["org.paymentauth/credential"]` per the [MCP transport spec](https://paymentauth.org/draft-payment-transport-mcp-00.html).

## Tests

```bash
cargo test --workspace                                    # 16 tests
cargo test -p zimppy-core --features shielded             # 19 tests (includes sapling decryption)
npx tsx --test packages/zimppy-ts/test/**/*.test.ts       # 14 tests
cargo clippy --workspace --all-targets -- -D warnings     # clean
npx tsc --noEmit                                          # clean
```

## Project structure

```
crates/
  zimppy-core/      Zcash verification engine (RPC, transparent, shielded, replay)
  zimppy-napi/      NAPI-RS bindings for Node.js
  zimppy-rs/        Rust MPP payment method
  zimppy-ports/     Shared port constants
packages/
  zimppy-ts/        TypeScript MPP payment method
apps/
  rust-server/      Example Rust server with 402 flow
  mcp-server/       MCP tool server with paid tools
  demo/             E2E demo script
research/           MPP spec analysis, Zcash capabilities, feasibility study
```

## What's next

- [ ] Full E2E with testnet ZEC (zebrad syncing, lightwalletd ready)
- [ ] Orchard pool support (in addition to Sapling)
- [ ] Session payments (prepaid balance with off-chain tracking)
- [ ] `mppx` integration (when API stabilizes)
- [ ] `mpp-rs` `ChargeMethod` trait integration (when crate is published)

## License

MIT
