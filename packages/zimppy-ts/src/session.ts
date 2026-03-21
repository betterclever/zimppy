/**
 * Zcash MPP Session — prepaid balance with off-chain bearer tokens.
 *
 * Flow:
 *   1. Client sends deposit (on-chain Orchard tx) → server creates session
 *   2. Client sends bearer token per request → server deducts balance (no on-chain tx)
 *   3. Client can topUp with more ZEC
 *   4. Client closes → server refunds unused balance
 *
 * Mirrors the solana-mpp session pattern.
 */

import { createHash, randomBytes } from 'node:crypto'
import { Credential, Method, Receipt, Store, z } from 'mppx'
import type { NapiCryptoClient } from './crypto-client.js'

// ── Session schemas ─────────────────────────────────────────────────

export const sessionRequestSchema = z.object({
  amount: z.string(),
  currency: z.string(),
  recipient: z.string(),
  network: z.enum(['testnet', 'mainnet']),
  depositAmount: z.optional(z.string()),
  idleTimeout: z.optional(z.number()),
  unitType: z.optional(z.string()),
  memo: z.optional(z.string()),
})

export const sessionCredentialPayloadSchema = z.discriminatedUnion('action', [
  z.object({
    action: z.literal('open'),
    depositTxid: z.string(),
    refundAddress: z.string(),
    bearerSecret: z.string(),
  }),
  z.object({
    action: z.literal('bearer'),
    sessionId: z.string(),
    bearer: z.string(),
  }),
  z.object({
    action: z.literal('topUp'),
    sessionId: z.string(),
    topUpTxid: z.string(),
  }),
  z.object({
    action: z.literal('close'),
    sessionId: z.string(),
    bearer: z.string(),
  }),
])

export const zcashSessionMethod = Method.from({
  name: 'zcash',
  intent: 'session',
  schema: {
    request: sessionRequestSchema,
    credential: {
      payload: sessionCredentialPayloadSchema,
    },
  },
})

// ── Session state ───────────────────────────────────────────────────

interface SessionState {
  sessionId: string
  bearerHash: string          // sha256 of deposit txid (the bearer secret)
  depositAmountZat: number    // total deposited (grows on topUp)
  spentZat: number            // total consumed
  refundAddress: string       // client's address for refund
  network: string
  status: 'active' | 'closing' | 'closed'
}

function sha256hex(input: string): string {
  return createHash('sha256').update(input).digest('hex')
}

// Per-session mutex
const sessionLocks = new Map<string, Promise<void>>()
async function withSessionLock<T>(sessionId: string, fn: () => Promise<T>): Promise<T> {
  const existing = sessionLocks.get(sessionId) ?? Promise.resolve()
  let resolve: () => void
  const next = new Promise<void>((r) => { resolve = r })
  sessionLocks.set(sessionId, next)
  try {
    await existing
    return await fn()
  } finally {
    resolve!()
    if (sessionLocks.get(sessionId) === next) {
      sessionLocks.delete(sessionId)
    }
  }
}

// ── Server session ──────────────────────────────────────────────────

export interface ZcashSessionServerOptions {
  /** Server's Orchard IVK for verifying deposits */
  orchardIvk: string
  /** NAPI crypto client for shielded verification */
  crypto: NapiCryptoClient
  /** Store for session state persistence */
  store: Store.Store
  /** Server address (for challenges) */
  recipient: string
  /** Network */
  network: 'testnet' | 'mainnet'
  /** Callback to send refund on close */
  sendRefund?: (params: { to: string; amountZat: number; memo: string }) => Promise<string>
}

export function zcashSession(options: ZcashSessionServerOptions) {
  const { orchardIvk, crypto, store, recipient, network } = options

  return Method.toServer(zcashSessionMethod, {
    async verify({ credential, request }) {
      const payload = credential.payload

      switch (payload.action) {
        case 'open':
          return handleOpen(payload, request)
        case 'bearer':
          return handleBearer(payload, request)
        case 'topUp':
          return handleTopUp(payload, request)
        case 'close':
          return handleClose(payload)
        default:
          throw new Error(`unknown session action: ${(payload as { action: string }).action}`)
      }
    },

    respond({ credential }) {
      const payload = credential.payload as z.output<typeof sessionCredentialPayloadSchema>
      // Management actions (open, topUp, close) return JSON directly
      // Bearer actions (content requests) return undefined → proceed to route handler
      if (payload.action === 'bearer') return undefined
      return new Response(JSON.stringify({ status: 'ok' }), {
        headers: { 'content-type': 'application/json' },
      })
    },
  })

  async function handleOpen(
    payload: { depositTxid: string; refundAddress: string; bearerSecret: string },
    request: z.output<typeof sessionRequestSchema>,
  ): Promise<Receipt.Receipt> {
    const { depositTxid, refundAddress, bearerSecret } = payload
    const consumedKey = `zcash-session:consumed:${depositTxid}`

    // Replay guard
    if (await store.get(consumedKey)) {
      throw new Error('deposit already consumed')
    }

    // Verify shielded deposit on-chain
    console.error(`[session:open] Verifying deposit txid=${depositTxid.slice(0, 16)}...`)
    const verifyResult = await crypto.verifyShielded({
      txid: depositTxid,
      orchardIvk,
      expectedChallengeId: '', // deposits don't need challenge binding
      expectedAmountZat: request.amount,
    })

    if (!verifyResult.verified || verifyResult.outputsDecrypted === 0) {
      throw new Error('deposit verification failed')
    }

    const depositAmount = Number(verifyResult.observedAmountZat)
    const chargeAmount = Number(request.amount)

    // Mark deposit consumed
    await store.put(consumedKey, true)

    // Create session
    const sessionId = `zs-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
    const bearerHash = sha256hex(bearerSecret)

    const state: SessionState = {
      sessionId,
      bearerHash,
      depositAmountZat: depositAmount,
      spentZat: 0, // nothing charged on open — billing starts on first bearer/stream use
      refundAddress,
      network,
      status: 'active',
    }

    await store.put(`zcash-session:${sessionId}`, state)
    console.error(`[session:open] Created session ${sessionId}, deposit=${depositAmount} zat, charged=${chargeAmount} zat`)

    return Receipt.from({
      method: 'zcash',
      status: 'success',
      reference: sessionId,
      timestamp: new Date().toISOString(),
    })
  }

  async function handleBearer(
    payload: { sessionId: string; bearer: string },
    request: z.output<typeof sessionRequestSchema>,
  ): Promise<Receipt.Receipt> {
    const { sessionId, bearer } = payload
    const chargeAmount = Number(request.amount)

    return withSessionLock(sessionId, async () => {
      const state = await store.get(`zcash-session:${sessionId}`) as SessionState | null
      if (!state) throw new Error('session not found')
      if (state.status !== 'active') throw new Error(`session is ${state.status}`)

      // Verify bearer
      if (sha256hex(bearer) !== state.bearerHash) {
        throw new Error('invalid bearer')
      }

      // Check balance
      const remaining = state.depositAmountZat - state.spentZat
      if (chargeAmount > remaining) {
        throw new Error(`insufficient balance: need ${chargeAmount}, have ${remaining}`)
      }

      // Deduct
      state.spentZat += chargeAmount
      await store.put(`zcash-session:${sessionId}`, state)

      console.error(`[session:bearer] ${sessionId}: charged ${chargeAmount}, remaining ${state.depositAmountZat - state.spentZat}`)

      return Receipt.from({
        method: 'zcash',
        status: 'success',
        reference: sessionId,
        timestamp: new Date().toISOString(),
      })
    })
  }

  async function handleTopUp(
    payload: { sessionId: string; topUpTxid: string },
    _request: z.output<typeof sessionRequestSchema>,
  ): Promise<Receipt.Receipt> {
    const { sessionId, topUpTxid } = payload
    const consumedKey = `zcash-session:topup-consumed:${topUpTxid}`

    return withSessionLock(sessionId, async () => {
      const state = await store.get(`zcash-session:${sessionId}`) as SessionState | null
      if (!state) throw new Error('session not found')
      if (state.status !== 'active') throw new Error(`session is ${state.status}`)

      // Replay guard
      if (await store.get(consumedKey)) {
        throw new Error('top-up already consumed')
      }

      // Verify shielded top-up on-chain
      console.error(`[session:topUp] Verifying top-up txid=${topUpTxid.slice(0, 16)}...`)
      const verifyResult = await crypto.verifyShielded({
        txid: topUpTxid,
        orchardIvk,
        expectedChallengeId: '',
        expectedAmountZat: '0', // accept any amount
      })

      if (!verifyResult.verified || verifyResult.outputsDecrypted === 0) {
        throw new Error('top-up verification failed')
      }

      const topUpAmount = Number(verifyResult.observedAmountZat)

      await store.put(consumedKey, true)
      state.depositAmountZat += topUpAmount
      await store.put(`zcash-session:${sessionId}`, state)

      console.error(`[session:topUp] ${sessionId}: added ${topUpAmount}, new balance ${state.depositAmountZat - state.spentZat}`)

      return Receipt.from({
        method: 'zcash',
        status: 'success',
        reference: sessionId,
        timestamp: new Date().toISOString(),
      })
    })
  }

  async function handleClose(
    payload: { sessionId: string; bearer: string },
  ): Promise<Receipt.Receipt> {
    const { sessionId, bearer } = payload

    return withSessionLock(sessionId, async () => {
      const state = await store.get(`zcash-session:${sessionId}`) as SessionState | null
      if (!state) throw new Error('session not found')
      if (state.status === 'closed') throw new Error('session already closed')

      // Verify bearer
      if (sha256hex(bearer) !== state.bearerHash) {
        throw new Error('invalid bearer')
      }

      // Transition to closing (prevents double-refund)
      state.status = 'closing'
      await store.put(`zcash-session:${sessionId}`, state)

      const refundAmount = state.depositAmountZat - state.spentZat
      console.error(`[session:close] ${sessionId}: refund=${refundAmount} zat to ${state.refundAddress.slice(0, 20)}...`)

      // Send refund if needed
      if (refundAmount > 0 && options.sendRefund) {
        try {
          const refundTxid = await options.sendRefund({
            to: state.refundAddress,
            amountZat: refundAmount,
            memo: `zimppy-refund:${sessionId}`,
          })
          console.error(`[session:close] Refund sent: ${refundTxid.slice(0, 16)}...`)
        } catch (err) {
          console.error(`[session:close] Refund failed: ${(err as Error).message}`)
          // Don't fail the close — the session is still finalized
        }
      }

      // Finalize
      state.status = 'closed'
      await store.put(`zcash-session:${sessionId}`, state)
      console.error(`[session:close] Session ${sessionId} closed. Total spent: ${state.spentZat} zat`)

      return Receipt.from({
        method: 'zcash',
        status: 'success',
        reference: sessionId,
        timestamp: new Date().toISOString(),
      })
    })
  }
}

/** @deprecated Use `zcashSession` instead */
export const zcashSessionServer = zcashSession

// ── Client session ──────────────────────────────────────────────────

export interface ZcashSessionClientOptions {
  /** Function to send a real Zcash payment and return the txid */
  sendPayment: (params: { to: string; amountZat: string; memo: string }) => Promise<string>
  /** Client's address for receiving refunds */
  refundAddress: string
}

export function zcashSessionClient(options: ZcashSessionClientOptions) {
  let activeSession: { sessionId: string; bearer: string } | null = null
  let pendingTopUp = false
  let pendingClose = false

  const client = Method.toClient(zcashSessionMethod, {
    async createCredential({ challenge }) {
      const { recipient, amount, memo } = challenge.request

      // Close
      if (pendingClose && activeSession) {
        const payload = {
          action: 'close' as const,
          sessionId: activeSession.sessionId,
          bearer: activeSession.bearer,
        }
        pendingClose = false
        const result = Credential.serialize(Credential.from({ challenge, payload }))
        activeSession = null
        return result
      }

      // TopUp
      if (pendingTopUp && activeSession) {
        console.error(`[session:client] Sending top-up of ${amount} zat...`)
        const topUpTxid = await options.sendPayment({
          to: recipient,
          amountZat: amount,
          memo: memo ?? '',
        })
        pendingTopUp = false
        return Credential.serialize(Credential.from({
          challenge,
          payload: {
            action: 'topUp' as const,
            sessionId: activeSession.sessionId,
            topUpTxid,
          },
        }))
      }

      // Bearer (hot path — no on-chain tx)
      if (activeSession) {
        return Credential.serialize(Credential.from({
          challenge,
          payload: {
            action: 'bearer' as const,
            sessionId: activeSession.sessionId,
            bearer: activeSession.bearer,
          },
        }))
      }

      // Open (first request — send deposit)
      const depositAmount = challenge.request.depositAmount ?? amount
      console.error(`[session:client] Opening session, depositing ${depositAmount} zat...`)
      const depositTxid = await options.sendPayment({
        to: recipient,
        amountZat: depositAmount,
        memo: memo ?? '',
      })

      // Generate a cryptographically random bearer secret (never exposed on-chain)
      const bearerSecret = randomBytes(32).toString('hex')
      activeSession = { sessionId: '', bearer: bearerSecret }

      return Credential.serialize(Credential.from({
        challenge,
        payload: {
          action: 'open' as const,
          depositTxid,
          refundAddress: options.refundAddress,
          bearerSecret,
        },
      }))
    },
  })

  return {
    ...client,

    /** Set the session ID after receiving the open receipt */
    setSessionId(id: string) {
      if (activeSession) activeSession.sessionId = id
    },

    /** Get current session info */
    getSession() {
      return activeSession ? { ...activeSession } : null
    },

    /** Mark next request as a top-up */
    topUp() { pendingTopUp = true },

    /** Mark next request as a close */
    close() { pendingClose = true },

    /** Get remaining balance (requires server cooperation — client doesn't track) */
    isActive() { return activeSession !== null },

    /** Restore session state from persisted storage */
    restore(sessionId: string, bearer: string) {
      activeSession = { sessionId, bearer }
    },

    /** Reset session state */
    cleanup() {
      activeSession = null
      pendingTopUp = false
      pendingClose = false
    },
  }
}
