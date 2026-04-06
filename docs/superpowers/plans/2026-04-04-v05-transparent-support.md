# v0.5 Transparent Address Support — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add full transparent (T-address) support to zimppy across the Rust wallet, NAPI bindings, TypeScript SDK, and CLI — including a new `zcash-transparent` payment method so servers can request transparent payments and clients can fulfill them.

**Architecture:** T-addresses are derived from the same wallet seed via zingolib's `generate_transparent_address` / `transparent_addresses_json`. A new `zcash-transparent` MPP method mirrors `zcash` but omits memo and uses on-chain transparent output verification. The `ZcashPaymentProvider` handles both shielded and transparent challenges. A `wallet shield` CLI command moves transparent funds into Orchard.

**Tech Stack:** Rust/zingolib (`zingolib_v3.0.1`), napi-rs, TypeScript, mppx

---

## File Map

| Action | File | What changes |
|--------|------|-------------|
| Modify | `crates/zimppy-wallet/src/error.rs` | Add `Shield(String)` variant |
| Modify | `crates/zimppy-wallet/src/lib.rs` | `WalletBalance` gains `transparent_zat`/`transparent_pending_zat`; add `transparent_address()` and `shield()` |
| Modify | `crates/zimppy-napi/src/lib.rs` | `NapiWalletBalance` gains `transparent_zat`/`transparent_pending_zat`; add `transparent_address()` and `shield()` bindings |
| Modify | `crates/zimppy-rs/src/provider.rs` | Support `zcash-transparent` in `supports()` + `pay()` |
| Modify | `packages/zimppy-ts/src/mppx.ts` | Add `zcashTransparentMethod`, `zcashTransparent()`, `zcashTransparentClient()` |
| Modify | `packages/zimppy-ts/src/wallet.ts` | Add `tAddress` to `ResolvedWallet`; call `transparentAddress()` in `resolveWallet()` |
| Modify | `packages/zimppy-ts/src/server.ts` | Add `zcashTransparent()` + re-export transparent symbols |
| Modify | `packages/zimppy-ts/src/index.ts` | Export new transparent client symbols |
| Modify | `packages/zimppy-cli/src/cli.ts` | Update `walletWhoami`/`walletBalance`; add `walletShield`; bump version to `0.5.0` |
| Modify | `Cargo.toml` + all `package.json` | Bump version `0.4.0` → `0.5.0` |

---

## Task 1: Add `Shield` error variant

**Files:**
- Modify: `crates/zimppy-wallet/src/error.rs`

- [ ] **Step 1: Add `Shield` variant**

Open `crates/zimppy-wallet/src/error.rs` and add after the `Send` variant:

```rust
#[error("shield error: {0}")]
Shield(String),
```

Full file after edit:
```rust
/// Wallet errors.
#[derive(Debug, thiserror::Error)]
pub enum WalletError {
    #[error("wallet error: {0}")]
    Client(String),
    #[error("wallet not initialized — run wallet login first")]
    NotInitialized,
    #[error("invalid seed phrase: {0}")]
    InvalidSeed(String),
    #[error("sync error: {0}")]
    Sync(String),
    #[error("send error: {0}")]
    Send(String),
    #[error("shield error: {0}")]
    Shield(String),
    #[error("address error: {0}")]
    Address(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("crypto error: {0}")]
    Crypto(String),
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo check -p zimppy-wallet 2>&1 | tail -5
```
Expected: no errors

- [ ] **Step 3: Commit**

```bash
git add crates/zimppy-wallet/src/error.rs
git commit -m "feat(wallet): add Shield error variant"
```

---

## Task 2: Transparent balance in `WalletBalance` + `balance()`

**Files:**
- Modify: `crates/zimppy-wallet/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)]` block in `crates/zimppy-wallet/src/lib.rs`:

```rust
#[test]
fn wallet_balance_has_transparent_fields() {
    let bal = WalletBalance {
        spendable_zat: 100,
        pending_zat: 0,
        total_zat: 100,
        transparent_zat: 50,
        transparent_pending_zat: 0,
    };
    assert_eq!(bal.transparent_zat, 50);
}
```

- [ ] **Step 2: Run test — expect compile failure**

```bash
cargo test -p zimppy-wallet wallet_balance_has_transparent_fields 2>&1 | tail -10
```
Expected: compile error — `WalletBalance` has no field `transparent_zat`

- [ ] **Step 3: Update `WalletBalance` struct**

In `crates/zimppy-wallet/src/lib.rs`, update the struct (around line 28):

```rust
/// Wallet balance information.
#[derive(Debug, Clone)]
pub struct WalletBalance {
    /// Spendable shielded (Orchard) balance in zatoshis
    pub spendable_zat: u64,
    /// Shielded balance pending confirmations
    pub pending_zat: u64,
    /// Total shielded balance (spendable + pending)
    pub total_zat: u64,
    /// Confirmed transparent balance in zatoshis
    pub transparent_zat: u64,
    /// Unconfirmed transparent balance in zatoshis
    pub transparent_pending_zat: u64,
}
```

- [ ] **Step 4: Update `balance()` to read transparent fields**

Replace the existing `balance()` method body (around line 267):

```rust
pub async fn balance(&self) -> Result<WalletBalance, WalletError> {
    let bal = self
        .client
        .account_balance(self.account_id())
        .await
        .map_err(|e| WalletError::Client(format!("{e}")))?;

    let spendable = bal.confirmed_orchard_balance.map(u64::from).unwrap_or(0);
    let pending = bal.unconfirmed_orchard_balance.map(u64::from).unwrap_or(0);
    let transparent = bal.confirmed_transparent_balance.map(u64::from).unwrap_or(0);
    let transparent_pending = bal.unconfirmed_transparent_balance.map(u64::from).unwrap_or(0);

    Ok(WalletBalance {
        spendable_zat: spendable,
        pending_zat: pending,
        total_zat: spendable + pending,
        transparent_zat: transparent,
        transparent_pending_zat: transparent_pending,
    })
}
```

- [ ] **Step 5: Run the test — expect pass**

```bash
cargo test -p zimppy-wallet wallet_balance_has_transparent_fields 2>&1 | tail -5
```
Expected: `test wallet_balance_has_transparent_fields ... ok`

- [ ] **Step 6: Verify full wallet test suite**

```bash
cargo test -p zimppy-wallet 2>&1 | tail -15
```
Expected: all existing tests still pass (unit tests — no live network needed)

- [ ] **Step 7: Commit**

```bash
git add crates/zimppy-wallet/src/lib.rs
git commit -m "feat(wallet): add transparent balance fields to WalletBalance"
```

---

## Task 3: Add `transparent_address()` to `ZimppyWallet`

**Files:**
- Modify: `crates/zimppy-wallet/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)]` block:

```rust
#[tokio::test]
async fn transparent_address_is_deterministic() {
    let data_dir = test_wallet_dir("t-addr");
    let seed = Mnemonic::<English>::generate(bip0039::Count::Words24).to_string();

    let mut wallet = ZimppyWallet::create(test_config(
        data_dir.clone(),
        Some(seed),
        Some(3_000_000),
    ))
    .await
    .expect("wallet should be created");

    let addr1 = wallet.transparent_address().await.expect("first call");
    let addr2 = wallet.transparent_address().await.expect("second call");
    // T-addresses on testnet start with "tm"
    assert!(addr1.starts_with("tm") || addr1.starts_with("t1"),
        "expected T-address, got: {addr1}");
    assert_eq!(addr1, addr2, "should be idempotent");

    let _ = fs::remove_dir_all(data_dir);
}
```

- [ ] **Step 2: Run test — expect compile failure**

```bash
cargo test -p zimppy-wallet transparent_address_is_deterministic 2>&1 | tail -10
```
Expected: compile error — `transparent_address` method not found

- [ ] **Step 3: Implement `transparent_address()`**

Add after `full_address()` in `crates/zimppy-wallet/src/lib.rs`:

```rust
/// Get (or generate) the wallet's first transparent T-address.
///
/// Calls `generate_transparent_address` on first use to ensure an address
/// is derived, then reads it back from `transparent_addresses_json`.
/// Subsequent calls return the same address (idempotent).
pub async fn transparent_address(&mut self) -> Result<String, WalletError> {
    // Check if a transparent address already exists
    let existing = self.client.transparent_addresses_json().await;
    let has_addr = existing
        .members()
        .next()
        .and_then(|a| a["encoded_address"].as_str())
        .is_some();

    if !has_addr {
        self.client
            .generate_transparent_address(self.account_id(), false)
            .await
            .map_err(|e| WalletError::Address(format!("failed to generate transparent address: {e}")))?;
        self.save().await?;
    }

    let addrs = self.client.transparent_addresses_json().await;
    addrs
        .members()
        .next()
        .and_then(|a| a["encoded_address"].as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| WalletError::Address("no transparent address found after generation".to_string()))
}
```

- [ ] **Step 4: Run test — expect pass**

```bash
cargo test -p zimppy-wallet transparent_address_is_deterministic 2>&1 | tail -5
```
Expected: `test transparent_address_is_deterministic ... ok`

- [ ] **Step 5: Run full suite**

```bash
cargo test -p zimppy-wallet 2>&1 | tail -10
```
Expected: all tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/zimppy-wallet/src/lib.rs
git commit -m "feat(wallet): add transparent_address() method"
```

---

## Task 4: Add `shield()` to `ZimppyWallet`

**Files:**
- Modify: `crates/zimppy-wallet/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)]` block:

```rust
#[tokio::test]
async fn shield_returns_error_when_no_transparent_funds() {
    let data_dir = test_wallet_dir("shield-empty");
    let seed = Mnemonic::<English>::generate(bip0039::Count::Words24).to_string();

    let mut wallet = ZimppyWallet::create(test_config(
        data_dir.clone(),
        Some(seed),
        Some(3_000_000),
    ))
    .await
    .expect("wallet should be created");

    // Shield with no funds should return a Shield error (nothing to shield)
    let result = wallet.shield().await;
    assert!(matches!(result, Err(WalletError::Shield(_))),
        "expected Shield error, got: {result:?}");

    let _ = fs::remove_dir_all(data_dir);
}
```

- [ ] **Step 2: Run test — expect compile failure**

```bash
cargo test -p zimppy-wallet shield_returns_error_when_no_transparent_funds 2>&1 | tail -10
```
Expected: compile error — `shield` method not found

- [ ] **Step 3: Implement `shield()`**

Add after `transparent_address()` in `crates/zimppy-wallet/src/lib.rs`:

```rust
/// Shield all transparent funds into the Orchard pool.
///
/// Uses zingolib's `quick_shield` which proposes and broadcasts in one step.
/// Returns the txid of the shielding transaction.
/// Errors if there are no transparent funds to shield.
pub async fn shield(&mut self) -> Result<String, WalletError> {
    trace_wallet(
        "shield:begin",
        format!("wallet_path={}", self.client.config().get_wallet_path().display()),
    );
    let txids = self
        .client
        .quick_shield(self.account_id())
        .await
        .map_err(|e| WalletError::Shield(format!("{e}")))?;

    // For encrypted wallets, force a save after the shielding tx is created
    if self.passphrase.is_some() {
        self.client.wallet.write().await.save_required = true;
        self.save().await?;
    }

    let txid = txids.head.to_string();
    trace_wallet("shield:end", format!("txid={txid}"));
    Ok(txid)
}
```

- [ ] **Step 4: Run test — expect pass**

```bash
cargo test -p zimppy-wallet shield_returns_error_when_no_transparent_funds 2>&1 | tail -5
```
Expected: `test shield_returns_error_when_no_transparent_funds ... ok`

- [ ] **Step 5: Run full suite**

```bash
cargo test -p zimppy-wallet 2>&1 | tail -10
```
Expected: all tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/zimppy-wallet/src/lib.rs
git commit -m "feat(wallet): add shield() method to move transparent funds to Orchard"
```

---

## Task 5: Update NAPI bindings

**Files:**
- Modify: `crates/zimppy-napi/src/lib.rs`

- [ ] **Step 1: Update `NapiWalletBalance` struct**

In `crates/zimppy-napi/src/lib.rs`, replace `NapiWalletBalance` (around line 119):

```rust
#[napi(object)]
pub struct NapiWalletBalance {
    pub spendable_zat: String,
    pub pending_zat: String,
    pub total_zat: String,
    pub transparent_zat: String,
    pub transparent_pending_zat: String,
}
```

- [ ] **Step 2: Update `balance()` NAPI method to pass new fields**

In the `balance()` method of `ZimppyWalletNapi` (around line 292):

```rust
#[napi]
pub async fn balance(&self) -> napi::Result<NapiWalletBalance> {
    self.ensure_open().await?;
    let wallet = self.wallet.lock().await;
    let bal = wallet
        .balance()
        .await
        .map_err(|e: WalletError| napi::Error::from_reason(e.to_string()))?;
    Ok(NapiWalletBalance {
        spendable_zat: bal.spendable_zat.to_string(),
        pending_zat: bal.pending_zat.to_string(),
        total_zat: bal.total_zat.to_string(),
        transparent_zat: bal.transparent_zat.to_string(),
        transparent_pending_zat: bal.transparent_pending_zat.to_string(),
    })
}
```

- [ ] **Step 3: Add `transparent_address()` NAPI method**

Add after `full_address()` in `ZimppyWalletNapi`:

```rust
/// Get (or generate) the wallet's first transparent T-address.
#[napi]
pub async fn transparent_address(&self) -> napi::Result<String> {
    self.ensure_open().await?;
    let mut wallet = self.wallet.lock().await;
    wallet
        .transparent_address()
        .await
        .map_err(|e| napi::Error::from_reason(e.to_string()))
}
```

- [ ] **Step 4: Add `shield()` NAPI method**

Add after `transparent_address()`:

```rust
/// Shield all transparent funds to Orchard. Returns the shielding txid.
#[napi]
pub async fn shield(&self) -> napi::Result<String> {
    self.ensure_open().await?;
    let mut wallet = self.wallet.lock().await;
    wallet
        .shield()
        .await
        .map_err(|e: WalletError| napi::Error::from_reason(e.to_string()))
}
```

- [ ] **Step 5: Build the NAPI package to verify**

```bash
cd crates/zimppy-napi && cargo check 2>&1 | tail -10
```
Expected: no errors

- [ ] **Step 6: Rebuild NAPI native module**

```bash
cd crates/zimppy-napi && cargo build --release 2>&1 | tail -5
```
Expected: `Finished release profile`

- [ ] **Step 7: Commit**

```bash
git add crates/zimppy-napi/src/lib.rs
git commit -m "feat(napi): expose transparent_address, shield, transparent balance fields"
```

---

## Task 6: Add `zcash-transparent` payment method to mppx.ts

**Files:**
- Modify: `packages/zimppy-ts/src/mppx.ts`

- [ ] **Step 1: Add transparent schemas, method definition, server and client functions**

Append to `packages/zimppy-ts/src/mppx.ts` (after the last line):

```typescript
// ── zcash-transparent ────────────────────────────────────────────────

export const zcashTransparentRequestSchema = z.object({
  amount: z.string(),
  currency: z.string(),
  recipient: z.string(), // Zcash T-address (tm... or t1...)
})

export const zcashTransparentCredentialPayloadSchema = z.object({
  txid: z.string(),
  outputIndex: z.number(),
})

export const zcashTransparentMethod = Method.from({
  name: 'zcash-transparent',
  intent: 'charge',
  schema: {
    request: zcashTransparentRequestSchema,
    credential: {
      payload: zcashTransparentCredentialPayloadSchema,
    },
  },
})

export interface ZcashTransparentVerifyResult {
  verified: boolean
  txid: string
  reference?: string
}

export interface ZcashTransparentServerOptions {
  /** T-address that will receive payments */
  tAddress?: string
  /** Zebrad RPC endpoint */
  rpcEndpoint?: string
  /** Override: custom verification function (skips NAPI verify) */
  verifyPayment?: (parameters: {
    amount: string
    challenge: z.output<typeof zcashTransparentRequestSchema>
    challengeId: string
    txid: string
    outputIndex: number
  }) => Promise<ZcashTransparentVerifyResult>
}

export function zcashTransparent(options: ZcashTransparentServerOptions) {
  const crypto = options.verifyPayment ? null : new NapiCryptoClient(options.rpcEndpoint)

  return Method.toServer(zcashTransparentMethod, {
    async verify({ credential, request }) {
      const { txid, outputIndex } = credential.payload

      const result = options.verifyPayment
        ? await options.verifyPayment({
            amount: request.amount,
            challenge: request,
            challengeId: credential.challenge.id,
            txid,
            outputIndex,
          })
        : await crypto!
            .verifyTransparent({
              txid,
              outputIndex,
              expectedAddress: request.recipient,
              expectedAmountZat: request.amount,
            })
            .then((r) => ({ verified: r.verified, txid: r.txid }))

      if (!result.verified) {
        throw new Error('payment not verified')
      }

      return Receipt.from({
        method: zcashTransparentMethod.name,
        status: 'success',
        timestamp: new Date().toISOString(),
        reference: result.reference ?? result.txid,
      })
    },
  })
}

export interface ZcashTransparentClientPayment {
  txid: string
  outputIndex: number
}

export interface ZcashTransparentClientOptions {
  createPayment?: (parameters: {
    challenge: z.output<typeof zcashTransparentRequestSchema>
    challengeId: string
  }) => Promise<ZcashTransparentClientPayment>
}

export function zcashTransparentClient(options: ZcashTransparentClientOptions = {}) {
  return Method.toClient(zcashTransparentMethod, {
    async createCredential({ challenge }) {
      if (!options.createPayment) {
        throw new Error(
          'zcash-transparent client auto-pay is not configured. Pass createPayment(...) to zcashTransparentClient().',
        )
      }

      const payment = await options.createPayment({
        challenge: challenge.request,
        challengeId: challenge.id,
      })

      return Credential.serialize({
        challenge,
        payload: {
          txid: payment.txid,
          outputIndex: payment.outputIndex,
        },
      })
    },
  })
}
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
cd packages/zimppy-ts && npx tsc --noEmit 2>&1 | head -20
```
Expected: no errors

- [ ] **Step 3: Commit**

```bash
git add packages/zimppy-ts/src/mppx.ts
git commit -m "feat(ts): add zcash-transparent payment method (mppx)"
```

---

## Task 7: Update `wallet.ts` — add `tAddress` to `ResolvedWallet`

**Files:**
- Modify: `packages/zimppy-ts/src/wallet.ts`

- [ ] **Step 1: Add `tAddress` to `ResolvedWallet` interface**

In `packages/zimppy-ts/src/wallet.ts`, update the `ResolvedWallet` interface:

```typescript
export interface ResolvedWallet {
  dataDir: string
  lwdServer: string
  rpcEndpoint: string
  network: 'testnet' | 'mainnet'
  address: string
  orchardIvk: string
  tAddress: string
}
```

- [ ] **Step 2: Update `resolveWallet()` to fetch T-address**

Replace the `Promise.all` block in `resolveWallet()`:

```typescript
const [address, orchardIvk, tAddress, walletNetwork] = await Promise.all([
  wallet.address(),
  wallet.orchardIvk(),
  wallet.transparentAddress(),
  Promise.resolve(wallet.network() as string),
]).finally(async () => {
  console.error('[zimppy-ts:resolveWallet:close]', { walletName, dataDir })
  await wallet.close().catch(() => {})
})

const network = (walletNetwork === 'mainnet' ? 'mainnet' : 'testnet') as 'testnet' | 'mainnet'

return {
  dataDir,
  lwdServer: config.lwdServer,
  rpcEndpoint: config.rpcEndpoint,
  network,
  address,
  orchardIvk,
  tAddress,
}
```

- [ ] **Step 3: Verify TypeScript compiles**

```bash
cd packages/zimppy-ts && npx tsc --noEmit 2>&1 | head -20
```
Expected: no errors

- [ ] **Step 4: Commit**

```bash
git add packages/zimppy-ts/src/wallet.ts
git commit -m "feat(ts): add tAddress to ResolvedWallet"
```

---

## Task 8: Update `server.ts` — expose transparent server method

**Files:**
- Modify: `packages/zimppy-ts/src/server.ts`

- [ ] **Step 1: Import transparent symbols at the top of `server.ts`**

Add to the existing import from `./mppx.js`:

```typescript
import {
  zcash as zcashRaw,
  zcashMethod,
  zcashRequestSchema,
  zcashCredentialPayloadSchema,
  zcashTransparent as zcashTransparentRaw,
  zcashTransparentMethod,
  zcashTransparentRequestSchema,
  zcashTransparentCredentialPayloadSchema,
  zcashTransparentClient,
} from './mppx.js'
import type { ZcashServerOptions, ZcashVerifyResult, ZcashTransparentServerOptions, ZcashTransparentVerifyResult } from './mppx.js'
```

- [ ] **Step 2: Add `ZcashTransparentOptions` interface and `zcashTransparent()` function**

Add after the `zcash.session` definition (around line 131):

```typescript
// ── Transparent Charge ───────────────────────────────────────────

export interface ZcashTransparentOptions {
  /** Wallet name in ~/.zimppy/wallets/. Uses active wallet if omitted. */
  wallet?: string
  /** Override: T-address that receives payments (skips wallet resolution) */
  tAddress?: string
  /** Override: Zebrad RPC endpoint */
  rpcEndpoint?: string
  /** Override: custom verification function */
  verifyPayment?: ZcashTransparentServerOptions['verifyPayment']
}

/**
 * Create a Zcash transparent charge method for the server.
 *
 * ```ts
 * const method = await zcashTransparent({ wallet: 'server-wallet' })
 * const mppx = Mppx.create({ methods: [method] })
 * ```
 */
export async function zcashTransparent(
  options: ZcashTransparentOptions = {},
): Promise<ReturnType<typeof zcashTransparentRaw>> {
  if (options.tAddress || options.verifyPayment) {
    return zcashTransparentRaw({
      tAddress: options.tAddress,
      rpcEndpoint: options.rpcEndpoint,
      verifyPayment: options.verifyPayment,
    })
  }

  const w = await resolveWallet(options.wallet)
  return zcashTransparentRaw({
    tAddress: w.tAddress,
    rpcEndpoint: w.rpcEndpoint,
  })
}
```

- [ ] **Step 3: Add transparent re-exports**

Append to the existing re-exports block at the bottom of `server.ts`:

```typescript
export {
  zcashTransparentRaw,
  zcashTransparentMethod,
  zcashTransparentRequestSchema,
  zcashTransparentCredentialPayloadSchema,
  zcashTransparentClient,
}
export type { ZcashTransparentServerOptions, ZcashTransparentVerifyResult }
```

- [ ] **Step 4: Verify TypeScript compiles**

```bash
cd packages/zimppy-ts && npx tsc --noEmit 2>&1 | head -20
```
Expected: no errors

- [ ] **Step 5: Commit**

```bash
git add packages/zimppy-ts/src/server.ts
git commit -m "feat(ts): add zcashTransparent() server method"
```

---

## Task 9: Update `index.ts` — export transparent client symbols

**Files:**
- Modify: `packages/zimppy-ts/src/index.ts`

- [ ] **Step 1: Check current exports**

```bash
grep -n "zcash\|client" packages/zimppy-ts/src/index.ts | head -20
```

- [ ] **Step 2: Add transparent client exports**

Find where `zcashClient` is exported in `packages/zimppy-ts/src/index.ts` and add the transparent equivalents alongside it:

```typescript
export {
  zcashTransparentMethod,
  zcashTransparentRequestSchema,
  zcashTransparentCredentialPayloadSchema,
  zcashTransparentClient,
} from './mppx.js'
export type { ZcashTransparentClientOptions, ZcashTransparentClientPayment } from './mppx.js'
```

- [ ] **Step 3: Verify TypeScript compiles**

```bash
cd packages/zimppy-ts && npx tsc --noEmit 2>&1 | head -20
```
Expected: no errors

- [ ] **Step 4: Commit**

```bash
git add packages/zimppy-ts/src/index.ts
git commit -m "feat(ts): export zcashTransparentClient from index"
```

---

## Task 10: Update `ZcashPaymentProvider` for transparent payments

**Files:**
- Modify: `crates/zimppy-rs/src/provider.rs`

- [ ] **Step 1: Write a failing test**

Add to `#[cfg(test)]` in `crates/zimppy-rs/src/provider.rs`:

```rust
#[test]
fn supports_zcash_transparent_charge() {
    let provider = ZcashPaymentProvider::new(
        WalletConfig {
            data_dir: PathBuf::from("/tmp/w"),
            lwd_endpoint: "https://testnet.zec.rocks".to_string(),
            network: NetworkType::Test,
            seed_phrase: None,
            birthday_height: None,
            account_index: 0,
            num_accounts: 1,
            passphrase: None,
        },
        "https://rpc.example.com",
    );
    assert!(provider.supports("zcash-transparent", "charge"));
    assert!(!provider.supports("zcash-transparent", "session"));
    // shielded still works
    assert!(provider.supports("zcash", "charge"));
}
```

- [ ] **Step 2: Run test — expect failure**

```bash
cargo test -p zimppy-rs supports_zcash_transparent_charge 2>&1 | tail -5
```
Expected: `FAILED` — `supports("zcash-transparent", "charge")` returns false

- [ ] **Step 3: Update `supports()` and `pay()` in provider.rs**

Replace the `PaymentProvider` impl block:

```rust
impl PaymentProvider for ZcashPaymentProvider {
    fn supports(&self, method: &str, intent: &str) -> bool {
        (method == "zcash" || method == "zcash-transparent") && intent == "charge"
    }

    async fn pay(&self, challenge: &PaymentChallenge) -> Result<PaymentCredential, MppError> {
        // Parse challenge request to get recipient, amount
        let request: serde_json::Value = challenge.request.decode().map_err(|e| {
            MppError::InvalidConfig(format!("failed to decode challenge request: {e}"))
        })?;

        let recipient = request["recipient"]
            .as_str()
            .ok_or_else(|| MppError::InvalidConfig("missing recipient in challenge".to_string()))?;
        let amount_str = request["amount"]
            .as_str()
            .ok_or_else(|| MppError::InvalidConfig("missing amount in challenge".to_string()))?;

        let amount_zat: u64 = amount_str
            .parse()
            .map_err(|_| MppError::InvalidConfig("invalid amount".to_string()))?;

        // Memo is present for shielded payments, absent for transparent
        let memo_opt = request["methodDetails"]["memo"]
            .as_str()
            .map(|m| m.replace("{id}", &challenge.id));
        let memo = memo_opt.as_deref().unwrap_or("");

        eprintln!("[ZcashProvider] Received 402 challenge:");
        eprintln!(
            "[ZcashProvider]   recipient: {}",
            &recipient[..20.min(recipient.len())]
        );
        eprintln!("[ZcashProvider]   amount: {} zat", amount_zat);
        if !memo.is_empty() {
            eprintln!("[ZcashProvider]   memo: {}", memo);
        }

        let txid = self.send_payment(recipient, amount_zat, memo).await?;
        self.wait_for_confirmation(&txid).await?;

        let echo = challenge.to_echo();
        let payload = PaymentPayload::hash(&txid);
        let mut credential = PaymentCredential::new(echo, payload);

        // For transparent payments (no memo), include outputIndex for verification.
        // For shielded payments, just the txid is needed.
        credential.payload = if memo_opt.is_some() {
            serde_json::json!({ "txid": txid })
        } else {
            serde_json::json!({ "txid": txid, "outputIndex": 0 })
        };

        eprintln!("[ZcashProvider] Credential ready with txid {}", &txid[..16.min(txid.len())]);
        Ok(credential)
    }
}
```

- [ ] **Step 4: Run the new test — expect pass**

```bash
cargo test -p zimppy-rs supports_zcash_transparent_charge 2>&1 | tail -5
```
Expected: `test supports_zcash_transparent_charge ... ok`

- [ ] **Step 5: Run full suite**

```bash
cargo test -p zimppy-rs 2>&1 | tail -10
```
Expected: all tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/zimppy-rs/src/provider.rs
git commit -m "feat(provider): support zcash-transparent payment method"
```

---

## Task 11: Update CLI

**Files:**
- Modify: `packages/zimppy-cli/src/cli.ts`

- [ ] **Step 1: Bump VERSION constant**

In `packages/zimppy-cli/src/cli.ts` (line 31), change:
```typescript
const VERSION = '0.4.0'
```
to:
```typescript
const VERSION = '0.5.0'
```

- [ ] **Step 2: Update `walletWhoami()` to show T-address**

In `walletWhoami()`, after the `address` line, add T-address fetch and display. Replace the console.log block:

```typescript
const address = await wallet.address()
const tAddress = await wallet.transparentAddress()
const shortAddr = address.length > 50 ? `${address.slice(0, 25)}...${address.slice(-15)}` : address

if (address && address !== cfg.address) {
  saveConfig({ ...cfg, address })
}

const bal = await wallet.balance()

console.log(`--- Zimppy Wallet ---`)
console.log(`  Address (UA):       ${shortAddr}`)
console.log(`  Address (T-addr):   ${tAddress}`)
console.log(`  Shielded balance:   ${bal.totalZat} zat`)
console.log(`  Transparent balance:${bal.transparentZat} zat`)
console.log(`  Network:  ${cfg.network}`)
console.log(`  Status:   Ready`)
console.log(`---`)
```

- [ ] **Step 3: Update `walletBalance()` to show transparent pool**

Replace the console.log block in `walletBalance()`:

```typescript
const bal = await wallet.balance()

console.log(`--- Wallet Balance ---`)
console.log(`  Shielded spendable: ${bal.spendableZat} zat`)
console.log(`  Shielded pending:   ${bal.pendingZat} zat`)
console.log(`  Shielded total:     ${bal.totalZat} zat`)
console.log(`  Transparent:        ${bal.transparentZat} zat`)
console.log(`  Transparent pending:${bal.transparentPendingZat} zat`)
console.log(`  Network:            ${cfg.network}`)
console.log(`---`)
```

- [ ] **Step 4: Add `walletShield()` function**

Add after `walletBalance()`:

```typescript
async function walletShield(): Promise<void> {
  const cfg = requireConfig()
  let wallet: ZimppyWalletNapi | null = null
  try {
    wallet = await openWallet(cfg)
    await syncWallet(wallet, 'Syncing wallet')

    const bal = await wallet.balance()
    if (BigInt(bal.transparentZat) === 0n) {
      console.log('No transparent funds to shield.')
      return
    }

    console.log(`Shielding ${bal.transparentZat} zat from transparent pool...`)
    const sp = ui.spinner('Shielding')
    try {
      const txid = await wallet.shield()
      sp.ok(`Shielded — txid: ${txid}`)
      console.log(`  txid: ${txid}`)
    } catch (e) {
      sp.fail('Shield failed', (e as Error).message)
    }
  } catch (e) {
    console.error(`ERROR: ${(e as Error).message}`)
  } finally {
    await wallet?.close().catch(() => {})
  }
}
```

- [ ] **Step 5: Wire `wallet shield` into the command dispatcher**

Find the command dispatch block (the `switch` or `if` chain for wallet subcommands) and add:

```typescript
case 'shield':
  await walletShield()
  break
```

Also update the help text at the top of the file comment to include:
```
 *   zimppy wallet shield         Shield transparent funds to Orchard pool
```

- [ ] **Step 6: Verify TypeScript compiles**

```bash
cd packages/zimppy-cli && npx tsc --noEmit 2>&1 | head -20
```
Expected: no errors

- [ ] **Step 7: Smoke-test the CLI help**

```bash
npx zimppy --help 2>&1 | head -20
```
Expected: help output shows without error

- [ ] **Step 8: Commit**

```bash
git add packages/zimppy-cli/src/cli.ts
git commit -m "feat(cli): show T-address, transparent balance; add wallet shield command"
```

---

## Task 12: Version bump to v0.5.0

**Files:**
- Modify: `Cargo.toml` (workspace)
- Modify: all `package.json` files

- [ ] **Step 1: Bump workspace Cargo version**

In `Cargo.toml` (root), change:
```toml
version = "0.4.0"
```
to:
```toml
version = "0.5.0"
```

- [ ] **Step 2: Bump all package.json files**

```bash
find /Users/betterclever/newprojects/experiments/zimppy/packages -name "package.json" -not -path "*/node_modules/*" | xargs grep -l '"version"'
```

For each file found, change `"version": "0.4.0"` to `"version": "0.5.0"`.

Also check the root package.json if it exists:
```bash
grep '"version"' /Users/betterclever/newprojects/experiments/zimppy/package.json 2>/dev/null
```

- [ ] **Step 3: Verify Cargo workspace version propagated**

```bash
cargo metadata --no-deps --format-version 1 | python3 -c "import sys,json; pkgs=json.load(sys.stdin)['packages']; [print(p['name'], p['version']) for p in pkgs]"
```
Expected: all internal crates show `0.5.0`

- [ ] **Step 4: Full Rust build check**

```bash
cargo check --workspace 2>&1 | tail -10
```
Expected: no errors

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml $(find packages -name "package.json" -not -path "*/node_modules/*")
git commit -m "chore: bump version to 0.5.0"
```

---

## Self-Review Checklist

**Spec coverage:**
- [x] T-address exposed at Rust, NAPI, TS, and CLI layers
- [x] Transparent balance shown in `balance()` and CLI
- [x] `shield()` command moves funds to Orchard
- [x] `zcash-transparent` MPP method with server + client
- [x] `ZcashPaymentProvider` handles both shielded and transparent challenges
- [x] `ResolvedWallet.tAddress` available to SDK consumers
- [x] Version bumped to 0.5.0

**Type consistency:**
- `transparent_address()` → Rust method → `transparentAddress()` NAPI → `wallet.transparentAddress()` TS → consistent
- `shield()` → Rust → NAPI → CLI `walletShield()` → consistent
- `transparent_zat` / `transparent_pending_zat` → `transparentZat` / `transparentPendingZat` NAPI camelCase → consistent
- `zcashTransparentMethod` / `zcashTransparent()` / `zcashTransparentClient()` → used consistently across mppx.ts, server.ts, index.ts

**No placeholders:** All code blocks are complete and runnable.
