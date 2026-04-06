# Multi-Account Infrastructure Plan

**Goal**: Per-challenge T-address generation (replay prevention) + account pool rotation (back-to-back send throughput).

**Branch**: `feat/transparent-address-support` (extends current v0.5 work)

---

## Background

### Problem 1: Replay attacks on transparent payments
Transparent txns have no memo field, so the server can't bind a payment to a specific challenge ID. If the server uses a single T-address, a client could reuse someone else's txid. **Fix**: generate a fresh T-address per challenge — the address itself becomes the challenge binding.

### Problem 2: Back-to-back send throughput
zingolib's `min_confirmations` is `NonZeroU32` (minimum 1) — 0-conf spends are architecturally impossible. A single account can only send once per block (~75s). **Fix**: rotate across N accounts, each with independent note pools.

### Problem 3: Session UX
Clients shouldn't know about multi-account internals. Session bearer tokens are tied to the session, not the account — the server rotates accounts transparently.

---

## Task 1: `generate_next_transparent_address()` in wallet

**File**: `crates/zimppy-wallet/src/lib.rs`

Add a new method that generates a *new* T-address each call (non-idempotent), unlike the existing `transparent_address()` which is idempotent (always returns index 0).

```rust
/// Generate the next transparent address (new BIP44 index each call).
/// Used for per-challenge address binding in transparent payments.
/// This is pure local HD derivation — no sync needed.
pub async fn generate_next_transparent_address(&mut self) -> Result<String, WalletError> {
    let (_, addr) = self.client
        .generate_transparent_address(self.account_id(), false)
        .await
        .map_err(|e| WalletError::Address(format!("failed to generate transparent address: {e}")))?;

    self.save().await?;

    Ok(addr.encode(&self.client.config().chain))
}
```

**Key facts**:
- `generate_transparent_address(account_id, enforce_no_gap)` is zingolib's method — `enforce_no_gap: false` means it always creates the next index
- Pure HD derivation from seed — no network call, no sync needed
- Existing `transparent_address()` stays unchanged (idempotent, index 0)

**Also add**: `transparent_addresses()` to list all generated T-addresses for an account:

```rust
/// List all transparent addresses generated for this account.
pub async fn transparent_addresses(&self) -> Result<Vec<String>, WalletError> {
    let addrs = self.client.transparent_addresses_json().await;
    Ok(addrs.members()
        .filter_map(|a| a["encoded_address"].as_str().map(String::from))
        .collect())
}
```

---

## Task 2: NAPI bindings for new wallet methods

**File**: `crates/zimppy-napi/src/lib.rs`

### 2a: Expose `generate_next_transparent_address`

```rust
#[napi]
pub async fn generate_next_transparent_address(&self) -> napi::Result<String> {
    self.ensure_open().await?;
    let mut wallet = self.wallet.lock().await;
    wallet
        .generate_next_transparent_address()
        .await
        .map_err(|e| napi::Error::from_reason(e.to_string()))
}
```

### 2b: Expose `transparent_addresses` (list all)

```rust
#[napi]
pub async fn transparent_addresses(&self) -> napi::Result<Vec<String>> {
    self.ensure_open().await?;
    let wallet = self.wallet.lock().await;
    wallet
        .transparent_addresses()
        .await
        .map_err(|e| napi::Error::from_reason(e.to_string()))
}
```

### 2c: Add `num_accounts` + `account_index` params to `open_inner`

Currently hardcoded:
```rust
// Current (line 217-218):
account_index: 0,
num_accounts: 1,
```

Change `open`, `create`, `restore` factory methods to accept optional `account_index` and `num_accounts`:

```rust
#[napi(factory)]
pub async fn open(
    data_dir: String,
    lwd_endpoint: String,
    network: String,
    passphrase: Option<String>,
    account_index: Option<u32>,   // NEW — defaults to 0
    num_accounts: Option<u32>,    // NEW — defaults to 1
) -> napi::Result<Self> { ... }
```

Pass through to `open_inner`. This is backwards-compatible — JS callers that don't pass the new args get `0` and `1`.

---

## Task 3: Per-challenge T-address in server

**File**: `packages/zimppy-ts/src/server.ts` (modify `zcashTransparent`)
**File**: `packages/zimppy-ts/src/mppx.ts` (modify `zcashTransparent` raw)

### Current flow
1. `resolveWallet()` opens wallet, reads single `tAddress`, closes wallet
2. `zcashTransparentMethod` uses `defaults: { recipient: tAddress }` — same address for every challenge

### New flow
1. `zcashTransparent()` opens wallet and **keeps it open** (no sync needed for HD derivation)
2. On each challenge, calls `wallet.generateNextTransparentAddress()` to get a fresh T-address
3. Sets that address as the challenge's `recipient` via mppx `defaults` per-challenge

### Implementation in `mppx.ts`

The mppx `Method.toServer` API supports a `defaults` function (not just static object). Change:

```ts
export interface ZcashTransparentServerOptions {
  tAddress?: string
  rpcEndpoint?: string
  verifyPayment?: (...)  => Promise<ZcashTransparentVerifyResult>
  // NEW: generate fresh address per challenge
  generateAddress?: () => Promise<string>
}

export function zcashTransparent(options: ZcashTransparentServerOptions) {
  const crypto = options.verifyPayment ? null : new NapiCryptoClient(options.rpcEndpoint)

  return Method.toServer(zcashTransparentMethod, {
    // If generateAddress provided, call it per-challenge; else use static tAddress
    ...(options.generateAddress
      ? { defaults: async () => ({ recipient: await options.generateAddress!() }) }
      : options.tAddress ? { defaults: { recipient: options.tAddress } } : {}),
    async verify({ credential, request }) {
      // ... same as current
    },
  })
}
```

> **Note**: Need to verify that mppx `defaults` supports async functions. If not, the alternative is to use a `beforeChallenge` hook or subclass. Check mppx types first.

### Implementation in `server.ts`

```ts
export async function zcashTransparent(options: ZcashTransparentOptions = {}) {
  if (options.tAddress || options.verifyPayment) {
    return zcashTransparentRaw({ /* ... static, same as today */ })
  }

  // Open wallet once, keep handle alive for address generation
  const { wallet } = await openWallet(options.wallet)

  return zcashTransparentRaw({
    rpcEndpoint: (await resolveWallet(options.wallet)).rpcEndpoint,
    generateAddress: () => wallet.generateNextTransparentAddress(),
  })
}
```

---

## Task 4: `AccountPool` in Rust provider

**File**: `crates/zimppy-rs/src/pool.rs` (new)
**File**: `crates/zimppy-rs/src/lib.rs` (re-export)

### Design

```rust
use zimppy_wallet::{WalletConfig, ZimppyWallet};

/// Pool of N wallet accounts for round-robin sending.
/// Each account has independent note pools, allowing back-to-back sends.
pub struct AccountPool {
    wallets: Vec<ZimppyWallet>,
    next: std::sync::atomic::AtomicUsize,
}

impl AccountPool {
    /// Create a pool of N accounts from the same seed.
    /// Opens the wallet N times with account_index 0..N-1.
    pub async fn new(base_config: WalletConfig, n: u32) -> Result<Self, WalletError> {
        let mut wallets = Vec::with_capacity(n as usize);
        for i in 0..n {
            let mut cfg = base_config.clone();
            cfg.account_index = i;
            cfg.num_accounts = n;
            let mut w = ZimppyWallet::open(cfg).await?;
            w.sync().await?;
            wallets.push(w);
        }
        Ok(Self {
            wallets,
            next: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    /// Get the next available account (round-robin).
    pub fn next_wallet(&self) -> &ZimppyWallet {
        let idx = self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % self.wallets.len();
        &self.wallets[idx]
    }

    /// Sync all accounts.
    pub async fn sync_all(&mut self) -> Result<(), WalletError> {
        for w in &mut self.wallets {
            w.sync().await?;
        }
        Ok(())
    }

    /// Get balances for all accounts.
    pub async fn balances(&self) -> Result<Vec<(u32, WalletBalance)>, WalletError> {
        let mut result = Vec::new();
        for (i, w) in self.wallets.iter().enumerate() {
            result.push((i as u32, w.balance().await?));
        }
        Ok(result)
    }
}
```

### Key considerations
- All N accounts share the same seed (ZIP32 HD derivation) but different `account_index`
- Each account has its own Orchard/transparent note pools
- Round-robin is simplest; could later upgrade to "pick account with spendable balance"
- The wallet file is shared — zingolib stores all accounts in one `zingo-wallet.dat`
- Need to verify: can we open the same wallet file N times with different `account_index`, or does zingolib handle multi-account within a single `LightClient`?

> **Investigation needed**: zingolib may already support multi-account within a single LightClient (the `no_of_accounts` parameter in `WalletBase::Mnemonic`). If so, `AccountPool` should use a single LightClient with `account_balance(account_id)` per account rather than N separate wallet opens. This is a critical design decision — **check zingolib's multi-account API before implementing**.

---

## Task 5: Update `ZcashPaymentProvider` to use `AccountPool`

**File**: `crates/zimppy-rs/src/provider.rs`

### Current problem
The provider opens a new wallet per `pay()` call (line 47), syncs it, sends, then drops it. This is extremely slow and doesn't support account rotation.

### New design

```rust
pub struct ZcashPaymentProvider {
    pool: Arc<TokioMutex<AccountPool>>,  // replaces wallet_config
    rpc_endpoint: String,
    confirmation_timeout: u64,
}

impl ZcashPaymentProvider {
    pub async fn new(wallet_config: WalletConfig, rpc_endpoint: &str, num_accounts: u32) -> Result<Self, MppError> {
        let pool = AccountPool::new(wallet_config, num_accounts).await
            .map_err(|e| MppError::InvalidConfig(format!("pool init failed: {e}")))?;
        Ok(Self {
            pool: Arc::new(TokioMutex::new(pool)),
            rpc_endpoint: rpc_endpoint.to_string(),
            confirmation_timeout: 300,
        })
    }
}
```

In `pay()`:
```rust
let mut pool = self.pool.lock().await;
let wallet = pool.next_wallet_mut();
// wallet is already synced — just send
let txid = wallet.send(recipient, amount_zat, memo).await?;
```

### Migration path
- Keep the old `ZcashPaymentProvider::new(config, rpc)` signature working with `num_accounts: 1` default
- Add `ZcashPaymentProvider::with_pool(config, rpc, n)` for multi-account

---

## Task 6: CLI `wallet balance --all`

**File**: `packages/zimppy-cli/src/cli.ts`

### Current behavior
`wallet balance` shows balance for account 0 only.

### New behavior
`wallet balance --all` shows per-account balances:

```
zimppy wallet balance --all
---
  Account 0:
    Shielded:    500000 zat
    Transparent: 100000 zat
  Account 1:
    Shielded:    300000 zat
    Transparent: 50000 zat
  ---
  Total:         950000 zat
---
```

### Implementation
1. Parse `--all` flag from args
2. If `--all`: open wallet with `num_accounts` from config, iterate `account_balance()` per account
3. Default (no flag): same as today — single account balance

This depends on NAPI exposing `num_accounts` config and per-account balance queries. Two approaches:
- **Option A**: Open wallet N times with different `account_index` (slow, N syncs)
- **Option B**: Expose zingolib's `account_balance(account_id)` via NAPI for arbitrary account IDs on a single wallet (preferred — single sync)

> **Prefer Option B**: Add a NAPI method `balanceForAccount(accountIndex: number)` that calls `self.client.account_balance(account_id)` directly.

---

## Implementation Order

```
Task 1 → Task 2 → Task 3 (can start in parallel with Task 4)
                    Task 4 → Task 5
                    Task 6 (after Task 2)
```

1. **Task 1** (wallet methods) — foundation, no deps
2. **Task 2** (NAPI bindings) — depends on Task 1
3. **Task 3** (per-challenge T-address) — depends on Task 2, highest user-facing value
4. **Task 4** (AccountPool) — depends on Task 1, needs zingolib investigation first
5. **Task 5** (provider update) — depends on Task 4
6. **Task 6** (CLI balance) — depends on Task 2

### Critical investigation before Task 4
- Does zingolib support multiple accounts within a single `LightClient`? Check `account_balance()`, `quick_send()` with different `account_id` args.
- If yes: AccountPool wraps a single LightClient, much simpler
- If no: AccountPool manages N wallet opens (heavier, but workable)

---

## Testing Strategy

- **Task 1**: Unit test — `generate_next_transparent_address()` returns different addresses on successive calls
- **Task 2**: Build NAPI, verify `generateNextTransparentAddress()` callable from JS
- **Task 3**: E2E — start transparent server, verify each challenge gets a different recipient address
- **Task 4**: Unit test — AccountPool round-robins correctly, balances aggregate
- **Task 5**: Integration — send two payments back-to-back, verify both succeed without waiting for confirmation
- **Task 6**: Manual — `zimppy wallet balance --all` shows per-account breakdown
