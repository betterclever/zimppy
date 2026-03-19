import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)

export interface VerifyTransparentRequest {
  txid: string
  outputIndex: number
  expectedAddress: string
  expectedAmountZat: string  // always string
}

export interface VerifyResult {
  verified: boolean
  txid: string
  observedAddress: string
  observedAmountZat: string
  confirmations: number
}

export interface VerifyShieldedRequest {
  txid: string
  orchardIvk: string
  expectedChallengeId: string
  expectedAmountZat: string
}

export interface ShieldedVerifyResult {
  verified: boolean
  txid: string
  observedAmountZat: string
  memoMatched: boolean
  outputsDecrypted: number
}

/** Interface for crypto verification backends. */
export interface CryptoBackend {
  verifyTransparent(req: VerifyTransparentRequest): Promise<VerifyResult>
  verifyShielded(req: VerifyShieldedRequest): Promise<ShieldedVerifyResult>
}

/**
 * NAPI-based crypto client — calls Rust verification natively (in-process).
 * Zero HTTP overhead. Preferred backend.
 */
export class NapiCryptoClient implements CryptoBackend {
  private core: InstanceType<typeof import('@zimppy/core-napi').ZimppyCore>

  constructor(rpcEndpoint: string = 'https://zcash-testnet-zebrad.gateway.tatum.io') {
    // Dynamic import of native module
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const { ZimppyCore } = require('@zimppy/core-napi') as typeof import('@zimppy/core-napi')
    this.core = new ZimppyCore(rpcEndpoint)
  }

  async verifyTransparent(req: VerifyTransparentRequest): Promise<VerifyResult> {
    const result = await this.core.verifyTransparent(
      req.txid,
      req.outputIndex,
      req.expectedAddress,
      req.expectedAmountZat,
    )
    return {
      verified: result.verified,
      txid: result.txid,
      observedAddress: result.observedAddress,
      observedAmountZat: result.observedAmountZat,
      confirmations: result.confirmations,
    }
  }

  async verifyShielded(req: VerifyShieldedRequest): Promise<ShieldedVerifyResult> {
    const result = await this.core.verifyShielded(
      req.txid,
      req.orchardIvk,
      req.expectedChallengeId,
      req.expectedAmountZat,
    )
    return {
      verified: result.verified,
      txid: result.txid,
      observedAmountZat: result.observedAmountZat,
      memoMatched: result.memoMatched,
      outputsDecrypted: result.outputsDecrypted,
    }
  }
}

/**
 * HTTP-based crypto client — calls zimppy-core-server via HTTP.
 * Fallback when NAPI is not available.
 */
export class HttpCryptoClient implements CryptoBackend {
  private endpoint: string

  constructor(endpoint: string = 'http://127.0.0.1:3181') {
    this.endpoint = endpoint
  }

  async verifyTransparent(req: VerifyTransparentRequest): Promise<VerifyResult> {
    const res = await fetch(`${this.endpoint}/verify/transparent`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        txid: req.txid,
        outputIndex: req.outputIndex,
        expectedAddress: req.expectedAddress,
        expectedAmountZat: req.expectedAmountZat,
      }),
    })
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: 'unknown error' }))
      throw new Error(`crypto server error ${res.status}: ${(err as Record<string, string>).error ?? 'unknown'}`)
    }
    const data = await res.json() as Record<string, unknown>
    return {
      verified: data.verified as boolean,
      txid: data.txid as string,
      observedAddress: (data.observed_address ?? data.observedAddress) as string,
      observedAmountZat: String(data.observed_amount_zat ?? data.observedAmountZat),
      confirmations: (data.confirmations ?? 0) as number,
    }
  }

  async verifyShielded(_req: VerifyShieldedRequest): Promise<ShieldedVerifyResult> {
    throw new Error('shielded verification is only supported through NAPI')
  }
}

/**
 * Create the best available crypto backend.
 * Tries NAPI first, falls back to HTTP.
 */
export function createCryptoBackend(options?: {
  rpcEndpoint?: string
  httpEndpoint?: string
  forceHttp?: boolean
}): CryptoBackend {
  if (options?.forceHttp) {
    return new HttpCryptoClient(options.httpEndpoint)
  }
  try {
    return new NapiCryptoClient(options?.rpcEndpoint)
  } catch {
    return new HttpCryptoClient(options?.httpEndpoint)
  }
}
