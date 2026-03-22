/**
 * zimppy-ts/client — Client-side Zcash payment method for MPP.
 *
 * Usage:
 *   import { zcash } from 'zimppy-ts/client'
 *
 *   // Simple — just a wallet name:
 *   const method = zcash({ wallet: 'default' })
 *
 *   // Advanced — custom payment function:
 *   const method = zcash({ createPayment: async ({ challenge, challengeId }) => ... })
 *
 *   const mppx = Mppx.create({ methods: [method] })
 */

import { openWallet } from './wallet.js'
import {
  zcashClient as zcashClientRaw,
  zcashMethod,
  zcashRequestSchema,
  zcashCredentialPayloadSchema,
} from './mppx.js'
import type { ZcashClientOptions, ZcashClientPayment } from './mppx.js'
import {
  zcashSessionClient as zcashSessionClientRaw,
  zcashSessionMethod,
  sessionRequestSchema,
  sessionCredentialPayloadSchema,
} from './session.js'
import type { ZcashSessionClientOptions } from './session.js'

// ── Charge ──────────────────────────────────────────────────────

export interface ZcashOptions {
  /** Wallet name in ~/.zimppy/wallets/. Uses active wallet if omitted. */
  wallet?: string
  /** Override: custom payment function (skips wallet) */
  createPayment?: ZcashClientOptions['createPayment']
  /** Optional payer identifier */
  source?: string
}

/**
 * Create a Zcash charge method for the client.
 *
 * ```ts
 * const method = zcash({ wallet: 'default' })
 * const mppx = Mppx.create({ methods: [method] })
 * const res = await mppx.fetch('https://api.example.com/fortune')
 * ```
 */
export function zcash(options: ZcashOptions = {}): ReturnType<typeof zcashClientRaw> {
  if (options.createPayment) {
    return zcashClientRaw({
      createPayment: options.createPayment,
      source: options.source,
    })
  }

  const walletName = options.wallet

  return zcashClientRaw({
    source: options.source,
    createPayment: async ({ challenge, challengeId }) => {
      const { wallet } = await openWallet(walletName)

      // Sync wallet before sending
      for (let i = 0; i < 10; i++) {
        if (await wallet.sync()) break
      }

      // Build memo from challenge template
      const memo = challenge.methodDetails?.memo
        ?.replace('{id}', challengeId) ?? challengeId

      const txid = await wallet.send(
        challenge.recipient,
        challenge.amount,
        memo,
      )

      // Post-send sync for shard tree
      for (let i = 0; i < 5; i++) {
        if (await wallet.sync()) break
      }

      return { txid }
    },
  })
}

// ── Session ─────────────────────────────────────────────────────

export interface ZcashSessionOptions {
  /** Wallet name in ~/.zimppy/wallets/. Uses active wallet if omitted. */
  wallet?: string
  /** Override: custom send function */
  sendPayment?: ZcashSessionClientOptions['sendPayment']
  /** Override: refund address */
  refundAddress?: string
}

/**
 * Create a Zcash session method for the client.
 *
 * ```ts
 * const method = zcash.session({ wallet: 'default' })
 * const mppx = Mppx.create({ methods: [method] })
 * ```
 */
zcash.session = function session(options: ZcashSessionOptions = {}) {
  if (options.sendPayment && options.refundAddress) {
    return zcashSessionClientRaw({
      sendPayment: options.sendPayment,
      refundAddress: options.refundAddress,
    })
  }

  const walletName = options.wallet
  let walletInstance: any = null

  async function getWallet() {
    if (!walletInstance) {
      const result = await openWallet(walletName)
      walletInstance = result.wallet
    }
    return walletInstance
  }

  return zcashSessionClientRaw({
    refundAddress: options.refundAddress ?? (async () => {
      const wallet = await getWallet()
      return wallet.address()
    }),
    sendPayment: async ({ to, amountZat, memo }) => {
      const wallet = await getWallet()

      for (let i = 0; i < 10; i++) {
        if (await wallet.sync()) break
      }

      const txid = await wallet.send(to, amountZat, memo)

      for (let i = 0; i < 5; i++) {
        if (await wallet.sync()) break
      }

      return txid
    },
  })
}

// ── Re-exports ──────────────────────────────────────────────────

export {
  zcashClientRaw,
  zcashSessionClientRaw,
  zcashMethod,
  zcashRequestSchema,
  zcashCredentialPayloadSchema,
  zcashSessionMethod,
  sessionRequestSchema,
  sessionCredentialPayloadSchema,
}
export type { ZcashClientOptions, ZcashClientPayment, ZcashSessionClientOptions }

export { parseEvent, isEventStream, iterateData } from './sse.js'
export type { SseEvent, NeedTopupEvent, StreamReceipt } from './sse.js'
