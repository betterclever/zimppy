import { createCryptoBackend } from './crypto-client.js'
import type { CryptoBackend } from './crypto-client.js'
import { ZcashChargeCredential, ZCASH_METHOD_NAME, ZCASH_CHARGE_INTENT } from './method.js'
import type { ZcashCredential, ZcashRequest } from './method.js'

export interface ZcashServerConfig {
  recipient: string
  network: 'testnet' | 'mainnet'
  /** Zebrad RPC endpoint for NAPI mode (default: Tatum testnet) */
  rpcEndpoint?: string
  /** HTTP endpoint for fallback mode (default: http://127.0.0.1:3181) */
  cryptoEndpoint?: string
  /** Force HTTP mode instead of NAPI */
  forceHttp?: boolean
}

export interface PaymentChallenge {
  scheme: 'Payment'
  method: string
  intent: string
  realm: string
  request: ZcashRequest & { challengeId: string; expiresAt: string }
}

export interface PaymentReceipt {
  status: 'success'
  method: string
  reference: string
  timestamp: string
}

export class ZcashChargeServer {
  private crypto: CryptoBackend
  private config: ZcashServerConfig
  private realm: string

  constructor(config: ZcashServerConfig) {
    this.config = config
    this.crypto = createCryptoBackend({
      rpcEndpoint: config.rpcEndpoint,
      httpEndpoint: config.cryptoEndpoint,
      forceHttp: config.forceHttp,
    })
    this.realm = 'zimppy'
  }

  /** Generate a 402 payment challenge for a given amount in zatoshis. */
  createChallenge(amountZat: string): PaymentChallenge {
    const challengeId = crypto.randomUUID()
    const expiresAt = new Date(Date.now() + 10 * 60 * 1000).toISOString() // 10 min TTL

    return {
      scheme: 'Payment',
      method: ZCASH_METHOD_NAME,
      intent: ZCASH_CHARGE_INTENT,
      realm: this.realm,
      request: {
        challengeId,
        amount: amountZat,
        currency: 'ZEC',
        recipient: this.config.recipient,
        network: this.config.network,
        expiresAt,
      },
    }
  }

  /** Format challenge as WWW-Authenticate header value. */
  formatWwwAuthenticate(challenge: PaymentChallenge): string {
    const encodedRequest = Buffer.from(JSON.stringify(challenge.request), 'utf8').toString('base64url')
    return [
      `Payment id="${challenge.request.challengeId}"`,
      `realm="${challenge.realm}"`,
      `method="${challenge.method}"`,
      `intent="${challenge.intent}"`,
      `request="${encodedRequest}"`,
    ].join(', ')
  }

  /** Parse and validate a Payment credential from the Authorization header. */
  parseCredential(authHeader: string): ZcashCredential {
    if (!authHeader.startsWith('Payment ')) {
      throw new Error('unsupported authorization scheme')
    }
    const encoded = authHeader.slice('Payment '.length).trim()
    const decoded = JSON.parse(Buffer.from(encoded, 'base64url').toString('utf8'))
    return ZcashChargeCredential.parse(decoded)
  }

  /** Verify a payment credential against expected terms. */
  async verify(credential: ZcashCredential, amountZat: string): Promise<PaymentReceipt> {
    const result = await this.crypto.verifyTransparent({
      txid: credential.payload.txid,
      outputIndex: credential.payload.outputIndex,
      expectedAddress: this.config.recipient,
      expectedAmountZat: amountZat,
    })

    if (!result.verified) {
      throw new Error(`payment verification failed for txid ${credential.payload.txid}`)
    }

    return {
      status: 'success',
      method: ZCASH_METHOD_NAME,
      reference: result.txid,
      timestamp: new Date().toISOString(),
    }
  }
}
