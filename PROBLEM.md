# Zingolib Sapling Checkpoint Bug — Investigation Brief

## The Problem

When using zingolib v3.0.0 (or v3.0.0-rc.6) programmatically via NAPI bindings, **every send fails** with:

```
Sapling shard store checkpoint not found at anchor height XXXXXXX
```

But **zingo-cli** (built against `zingolib_v3.0.0-rc.6`) works perfectly with the same wallet seed, same lightwalletd server, same everything.

## What Works vs What Doesn't

| Scenario | Result |
|----------|--------|
| zingo-cli `quicksend` (interactive REPL, `--waitsync`) | **WORKS** |
| zingo-cli one-shot command with `--waitsync` | **WORKS** |
| zcash-devtool send (uses `zcash_client_sqlite`, not zingolib) | **WORKS** |
| Our NAPI: open wallet → `sync_and_await()` → `quick_send()` | **FAILS** |
| Our NAPI: open wallet → `sync()` x5 → `quick_send()` | **FAILS** |
| Our NAPI: keep instance alive → sync → send → sync → send | First send works, second fails with "insufficient balance" (expected — unconfirmed change), **no checkpoint error** |

Wait — that last point is key. When we keep the wallet instance alive in one Node.js process:
- Open wallet → sync → send #1 → **SUCCESS**
- sync → send #2 → fails with "insufficient balance" (expected, change unconfirmed)
- **NO checkpoint error** in this case

But when we open a fresh wallet instance (new Node process or new `open()` call):
- Open wallet → sync → send → **FAILS with Sapling checkpoint error**

## The Suspected Root Cause

Zingolib does NOT serialize shard trees to the `.dat` wallet file. When you `open()` a wallet from disk:
1. Shard trees start EMPTY (`ShardTrees::new()`)
2. `sync()` calls `pepper_sync::sync()` which fetches subtree roots and scans blocks
3. During scanning, `set_checkpoint_retentions()` creates checkpoints for each pool — BUT only if the block has outputs in that pool
4. Most testnet blocks have Orchard outputs but no Sapling outputs
5. So Orchard gets checkpoints, Sapling doesn't
6. When `quick_send()` proposes a transaction, `get_target_and_anchor_heights()` computes the anchor using the Sapling tree's max checkpoint (stale/missing)
7. `spendable_notes()` checks both pool checkpoints at that anchor → Sapling check fails

## Why zingo-cli Works

**Unknown.** zingo-cli uses the exact same code path:
- `create_from_wallet_path()` → reads `.dat` → empty shard trees
- `sync()` → pepper_sync::sync()
- `await_sync()` → waits for completion
- `quick_send()` → same proposal/send path

zingo-cli binary is built from `zingolib_v3.0.0-rc.6` (confirmed via `strings` on the binary). We tested both v3.0.0 and v3.0.0-rc.6 — both fail in our code.

**The question: what does zingo-cli do differently that we're missing?**

## File Locations

### Our code
- **zimppy-wallet (Rust)**: `/Users/betterclever/newprojects/experiments/zimppy/crates/zimppy-wallet/src/lib.rs`
- **NAPI bindings**: `/Users/betterclever/newprojects/experiments/zimppy/crates/zimppy-napi/src/lib.rs`
- **CLI (TypeScript)**: `/Users/betterclever/newprojects/experiments/zimppy/packages/zimppy-cli/src/cli.ts`
- **Cargo.toml (workspace)**: `/Users/betterclever/newprojects/experiments/zimppy/Cargo.toml`
- **Wallet Cargo.toml**: `/Users/betterclever/newprojects/experiments/zimppy/crates/zimppy-wallet/Cargo.toml`

### Zingolib source (v3.0.0)
- **Root**: `/Users/betterclever/.cargo/git/checkouts/zingolib-23528b9eadeccb15/8a663aa/`
- **LightClient (sync, send)**: `zingolib/src/lightclient/sync.rs`, `zingolib/src/lightclient/send.rs`
- **LightWallet (shard trees, serialization)**: `zingolib/src/wallet.rs`
- **Spendable notes (THE BUG)**: `zingolib/src/wallet/output.rs` lines 250-275
- **Anchor height calc**: `zingolib/src/wallet/zcb_traits.rs` lines 184-211
- **pepper-sync scan (checkpoint creation)**: `pepper-sync/src/scan/compact_blocks.rs` lines 138-145, 479-501
- **pepper-sync sync (subtree roots)**: `pepper-sync/src/sync.rs` lines 1674-1729
- **zingo-cli**: `zingo-cli/src/lib.rs`, `zingo-cli/src/commands.rs`

### Patched zingolib (our workaround)
- `/tmp/zingolib-patched/zingolib/src/wallet/output.rs` — returns `Ok(vec![])` instead of error on missing checkpoint
- `/tmp/zingolib-patched/zingolib/src/wallet/zcb_traits.rs` — uses `max(sapling, orchard)` for anchor height
- Patch file: `/Users/betterclever/newprojects/experiments/zimppy/patches/zingolib-checkpoint-fix.patch`

### zcash-devtool (working reference)
- **Source**: `/Users/betterclever/.cargo/git/checkouts/zcash-devtool-d1de204deb056935/552c472/`
- **Sync implementation**: `src/commands/wallet/sync.rs` (687 lines)
- **Working wallet**: `/tmp/zcdt-wallet/` (data.sqlite, blockmeta.sqlite)

### Test wallets
- **~/.zimppy/wallets/rc6-test/** — fresh wallet created with rc.6, funded with 2 ZEC, FAILS to send
- **~/.zimppy/wallets/test-ua/** — wallet with Sapling+Orchard UA, funded, FAILS to send
- **/tmp/zingo-fresh/** — zingo-cli wallet, same seed, WORKS

## Key Code Paths

### Our send flow (FAILS):
```
NAPI: wallet.send(to, amount, memo)
  → ZimppyWallet::send()
    → LightClient::quick_send(request, account, true)
      → propose_send(request, account)  [proposes tx]
        → zcash_client_backend::propose_transfer()
          → InputSource::select_spendable_notes()  [selects notes to spend]
            → LightWallet::spendable_notes::<SaplingNote>(anchor_height, ...)
              → get_checkpoint(&anchor_height) → NONE → ERROR
```

### zingo-cli send flow (WORKS):
```
zingo-cli --waitsync quicksend <addr> <amount>
  → lib.rs: sync("run") + await_sync()  [full sync]
  → commands.rs: lightclient.quick_send(request, account, true)  [SAME as ours]
    → propose_send(request, account)
      → zcash_client_backend::propose_transfer()
        → InputSource::select_spendable_notes()
          → LightWallet::spendable_notes::<SaplingNote>(anchor_height, ...)
            → get_checkpoint(&anchor_height) → ??? SOMEHOW HAS IT ???
```

## Upstream Issues
- **zingolabs/zingolib#2015** — "Error sending transaction" — exact same error, reported by zingolib maintainer
- **zingolabs/zingolib#2267** — our issue filing
- **zingolabs/zingolib#2186** — "A Sapling Prover is created even when no sapling spends"

## Critical Clue: Wallet File Sizes

```
zingo-cli wallet (WORKS):  75,422 bytes
Our rc6-test (FAILS):      25,784 bytes
Our test-ua (FAILS):       48,601 bytes
```

Zingo-cli's wallet is 3x larger. The extra ~50K is likely shard tree data. Zingo-cli runs a background save task (`save("run")`) that periodically saves wallet state WHILE sync is running. Our code only calls `save()` after sync completes — the shard trees may not be marked as `save_required` by pepper-sync.

**Hypothesis:** zingo-cli's background save captures shard tree state mid-sync, so on reload the trees are partially populated. Our save-after-sync misses the shard tree data because `save_required` isn't set by the shard tree updates.

## Questions for Investigation

1. **What does zingo-cli's sync do that our `sync_and_await()` doesn't?** Both call `pepper_sync::sync()`. Is there a difference in the ZingoConfig, WalletSettings, or SyncConfig?

2. **Does zingo-cli's wallet file contain shard tree data?** Compare `/tmp/zingo-fresh/zingo-wallet.dat` (created by zingo-cli, 600K+) vs our wallet files (~14K). If zingo-cli's is much bigger, maybe shard trees ARE serialized in rc.6.

3. **Is the wallet file format different between v3.0.0 and rc.6?** We used v3.0.0 tag but zingo-cli uses rc.6. Maybe rc.6 added shard tree serialization.

4. **What ZingoConfig does zingo-cli use?** Look at `zingo-cli/src/lib.rs` `startup()` function — what SyncConfig, wallet_settings, etc. are passed?

5. **Is there a save-after-sync that persists shard trees?** zingo-cli does `sync("run")` then `save("run")` then `await_sync()`. Maybe the save after sync start but before await captures something?
