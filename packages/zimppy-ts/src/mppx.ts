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
