# crates/

Rust libraries powering Zimppy's Zcash payment verification and protocol implementation.

## zimppy-core

Low-level Zcash verification engine. Handles Orchard shielded transaction decryption, memo extraction, replay protection, and RPC communication with Zebrad.

Key capabilities:
- Decrypt Orchard outputs using an Incoming Viewing Key (IVK)
- Extract and verify challenge-bound memos (`zimppy:{challengeId}`)
- Replay protection via consumed txid tracking
- BIP39 mnemonic to Orchard IVK derivation (behind `keygen` feature flag)

## zimppy-rs

High-level Rust SDK for building MPP servers. Provides ready-to-use payment method implementations.

- **`ZcashChargeMethod`** — one-time payment verification (HTTP 402 flow)
- **`ZcashSessionMethod`** — prepaid deposit sessions with bearer tokens, balance tracking, and refunds
- **`ZcashPaymentProvider`** — client-side payment orchestration (sync, send, confirm)
- **`sse`** — Server-Sent Events streaming with per-token deduction

## zimppy-napi

NAPI-RS bindings exposing `zimppy-core` to Node.js. Used by `packages/zimppy-ts` for native Zcash verification in TypeScript applications.
