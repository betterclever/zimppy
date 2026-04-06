import { Credential, Method, Receipt, z } from 'mppx'
import { NapiCryptoClient } from './crypto-client.js'

export const zcashRequestSchema = z.object({
  amount: z.string(),
  currency: z.string(),
  recipient: z.string(),
  methodDetails: z.optional(z.object({
    network: z.optional(z.enum(['testnet', 'mainnet'])),
    memo: z.optional(z.string()),
  })),
})

export const zcashCredentialPayloadSchema = z.object({
  txid: z.string(),
})

export const zcashMethod = Method.from({
  name: 'zcash',
  intent: 'charge',
  schema: {
    request: zcashRequestSchema,
    credential: {
      payload: zcashCredentialPayloadSchema,
    },
  },
})

export interface ZcashVerifyResult {
  verified: boolean
  txid: string
  reference?: string
}

export interface ZcashServerOptions {
  orchardIvk?: string
  rpcEndpoint?: string
  verifyPayment?: (parameters: {
    amount: string
    challenge: z.output<typeof zcashRequestSchema>
    challengeId: string
    txid: string
  }) => Promise<ZcashVerifyResult>
}

export function zcash(options: ZcashServerOptions) {
  const crypto = options.verifyPayment ? null : new NapiCryptoClient(options.rpcEndpoint)

  return Method.toServer(zcashMethod, {
    async verify({ credential, request }) {
      const txid = credential.payload.txid
      const challengeId = credential.challenge.id
      const amount = request.amount

      const result = options.verifyPayment
        ? await options.verifyPayment({ amount, challenge: request, challengeId, txid })
        : await verifyViaNapi({
            amount,
            challengeId,
            crypto: crypto!,
            orchardIvk: options.orchardIvk,
            txid,
          })

      if (!result.verified) {
        throw new Error('payment not verified')
      }

      return Receipt.from({
        method: zcashMethod.name,
        status: 'success',
        timestamp: new Date().toISOString(),
        reference: result.reference ?? result.txid,
      })
    },
  })
}

export interface ZcashClientPayment {
  source?: string
  txid: string
}

export interface ZcashClientOptions {
  createPayment?: (parameters: {
    challenge: z.output<typeof zcashRequestSchema>
    challengeId: string
  }) => Promise<ZcashClientPayment>
  source?: string
}

export function zcashClient(options: ZcashClientOptions = {}) {
  return Method.toClient(zcashMethod, {
    async createCredential({ challenge }) {
      if (!options.createPayment) {
        throw new Error(
          'zcash client auto-pay is not configured. Pass createPayment(...) to zcashClient() to send a real payment and return a txid.',
        )
      }

      const payment = await options.createPayment({ challenge: challenge.request, challengeId: challenge.id })

      return Credential.serialize({
        challenge,
        payload: {
          txid: payment.txid,
        },
        ...(payment.source ?? options.source ? { source: payment.source ?? options.source } : {}),
      })
    },
  })
}

async function verifyViaNapi(parameters: {
  amount: string
  challengeId: string
  crypto: NapiCryptoClient
  orchardIvk?: string
  txid: string
}): Promise<ZcashVerifyResult> {
  const { amount, challengeId, crypto, orchardIvk, txid } = parameters

  if (!orchardIvk) {
    throw new Error('orchardIvk is required when verifyPayment is not provided')
  }

  const result = await crypto.verifyShielded({
    txid,
    orchardIvk,
    expectedChallengeId: challengeId,
    expectedAmountZat: amount,
  })
  return {
    verified: result.verified,
    txid: result.txid,
  }
}

/** @deprecated Use `zcash` instead */
export const zcashServer = zcash

export { zcashMethod as method }

// ── zcashtransparent ────────────────────────────────────────────────

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
  name: 'zcashtransparent',
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
  /** Generate a fresh T-address per challenge (for replay prevention) */
  generateAddress?: () => Promise<string>
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
    ...(options.tAddress && !options.generateAddress ? { defaults: { recipient: options.tAddress } } : {}),
    // When generateAddress is provided, inject a fresh T-address per challenge via the request hook
    ...(options.generateAddress ? {
      async request({ credential, request }: { credential?: unknown; request: z.input<typeof zcashTransparentRequestSchema> }) {
        // Only inject address on challenge creation (no credential yet), not on verification
        if (!credential && !request.recipient) {
          const freshAddress = await options.generateAddress!()
          return { ...request, recipient: freshAddress }
        }
        return request
      },
    } : {}),
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
            .then((r): ZcashTransparentVerifyResult => ({ verified: r.verified, txid: r.txid }))

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
          'zcashtransparent client auto-pay is not configured. Pass createPayment(...) to zcashTransparentClient().',
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
