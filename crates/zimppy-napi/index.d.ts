export interface NapiVerifyResult {
  verified: boolean
  txid: string
  observedAddress: string
  observedAmountZat: string
  confirmations: number
}

export declare class ZimppyCore {
  constructor(rpcEndpoint: string)
  verifyTransparent(
    txid: string,
    outputIndex: number,
    expectedAddress: string,
    expectedAmountZat: string,
  ): Promise<NapiVerifyResult>
  health(): Promise<string>
}
