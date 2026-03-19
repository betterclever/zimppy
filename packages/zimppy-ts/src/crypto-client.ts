export interface VerifyTransparentRequest {
  txid: string
  outputIndex: number
  expectedAddress: string
  expectedAmountZat: string  // always string
}

export interface VerifyResult {
  verified: boolean
  txid: string
  observed_address: string
  observed_amount_zat: number
  confirmations: number
}

export class CryptoClient {
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
    return res.json() as Promise<VerifyResult>
  }

  async health(): Promise<{ service: string; status: string }> {
    const res = await fetch(`${this.endpoint}/health`)
    return res.json() as Promise<{ service: string; status: string }>
  }
}
