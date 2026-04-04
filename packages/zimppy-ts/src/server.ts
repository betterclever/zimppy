/**
 * zimppy-ts/server — Server-side Zcash payment method for MPP.
 *
 * Usage:
 *   import { zcash } from 'zimppy-ts/server'
 *
 *   // Simple — just a wallet name:
 *   const method = await zcash({ wallet: 'server-wallet' })
 *
 *   // Advanced — raw options:
 *   const method = await zcash({ orchardIvk: '...', rpcEndpoint: '...' })
 *
 *   const mppx = Mppx.create({ methods: [method] })
 */

import { Store } from 'mppx'
import { resolveWallet } from './wallet.js'
import { NapiCryptoClient } from './crypto-client.js'
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
import {
  zcashSession as zcashSessionRaw,
  zcashSessionMethod,
  sessionRequestSchema,
  sessionCredentialPayloadSchema,
} from './session.js'
import type { ZcashSessionServerOptions } from './session.js'

// ── Charge ──────────────────────────────────────────────────────

export interface ZcashOptions {
  /** Wallet name in ~/.zimppy/wallets/. Uses active wallet if omitted. */
  wallet?: string
  /** Override: network (auto-detected from wallet if omitted) */
  network?: 'testnet' | 'mainnet'
  /** Override: Orchard Incoming Viewing Key (skips wallet resolution) */
  orchardIvk?: string
  /** Override: Zebrad RPC endpoint */
  rpcEndpoint?: string
  /** Override: custom verification function */
  verifyPayment?: ZcashServerOptions['verifyPayment']
}

/**
 * Create a Zcash charge method for the server.
 *
 * ```ts
 * const method = await zcash({ wallet: 'server-wallet' })
 * ```
 */
export async function zcash(options: ZcashOptions = {}): Promise<ReturnType<typeof zcashRaw>> {
  if (options.orchardIvk || options.verifyPayment) {
    return zcashRaw({
      orchardIvk: options.orchardIvk,
      rpcEndpoint: options.rpcEndpoint,
      verifyPayment: options.verifyPayment,
    })
  }

  const w = await resolveWallet(options.wallet)
  return zcashRaw({
    orchardIvk: w.orchardIvk,
    rpcEndpoint: w.rpcEndpoint,
  })
}

// ── Session ─────────────────────────────────────────────────────

export interface ZcashSessionOptions {
  /** Wallet name in ~/.zimppy/wallets/. Uses active wallet if omitted. */
  wallet?: string
  /** Store for session state. Defaults to in-memory. */
  store?: Store.Store
  /** Suggested deposit amount in zatoshis */
  suggestedDeposit?: number
  /** Label for the unit being priced */
  unitType?: string
  /** Callback to send refund on close */
  sendRefund?: ZcashSessionServerOptions['sendRefund']
  /** Override: Orchard IVK (skips wallet resolution) */
  orchardIvk?: string
  /** Override: Zebrad RPC endpoint */
  rpcEndpoint?: string
  /** Override: recipient address */
  recipient?: string
  /** Override: network */
  network?: 'testnet' | 'mainnet'
}

/**
 * Create a Zcash session method for the server.
 *
 * ```ts
 * const method = await zcash.session({ wallet: 'server-wallet', unitType: 'token' })
 * ```
 */
zcash.session = async function session(options: ZcashSessionOptions = {}) {
  let orchardIvk: string
  let rpcEndpoint: string
  let recipient: string
  let network: 'testnet' | 'mainnet'

  if (options.orchardIvk && options.recipient) {
    orchardIvk = options.orchardIvk
    rpcEndpoint = options.rpcEndpoint ?? ''
    recipient = options.recipient
    network = options.network ?? 'testnet'
  } else {
    const w = await resolveWallet(options.wallet)
    orchardIvk = w.orchardIvk
    rpcEndpoint = w.rpcEndpoint
    recipient = w.address
    network = w.network
  }

  return zcashSessionRaw({
    orchardIvk,
    crypto: new NapiCryptoClient(rpcEndpoint),
    store: options.store ?? Store.memory(),
    recipient,
    network,
    suggestedDeposit: options.suggestedDeposit,
    unitType: options.unitType,
    sendRefund: options.sendRefund,
  })
}

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

// ── Re-exports ──────────────────────────────────────────────────

export {
  zcashRaw,
  zcashSessionRaw,
  zcashMethod,
  zcashRequestSchema,
  zcashCredentialPayloadSchema,
  zcashSessionMethod,
  sessionRequestSchema,
  sessionCredentialPayloadSchema,
  zcashTransparentRaw,
  zcashTransparentMethod,
  zcashTransparentRequestSchema,
  zcashTransparentCredentialPayloadSchema,
  zcashTransparentClient,
}
export type { ZcashServerOptions, ZcashVerifyResult, ZcashSessionServerOptions, ZcashTransparentServerOptions, ZcashTransparentVerifyResult }

export { NapiCryptoClient, HttpCryptoClient, createCryptoBackend } from './crypto-client.js'
export type { CryptoBackend, ShieldedVerifyResult, VerifyShieldedRequest, VerifyTransparentRequest, VerifyResult } from './crypto-client.js'

export { serveStream, toResponse } from './sse.js'
export type { ServeStreamOptions, NeedTopupEvent, StreamReceipt } from './sse.js'

export {
  SessionNotFoundError,
  SessionClosedError,
  InsufficientBalanceError,
  InvalidBearerError,
  DepositConsumedError,
  DepositVerificationError,
  TopUpConsumedError,
} from './errors.js'
export type { SessionError } from './errors.js'
