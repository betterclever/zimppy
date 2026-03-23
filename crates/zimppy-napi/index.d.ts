export declare class ZimppyCore {
  constructor(rpcEndpoint: string)
  verifyTransparent(txid: string, outputIndex: number, expectedAddress: string, expectedAmountZat: string): Promise<NapiVerifyResult>
  verifyShielded(txid: string, orchardIvk: string, expectedChallengeId: string, expectedAmountZat: string): Promise<NapiShieldedVerifyResult>
  health(): Promise<string>
}

export interface NapiVerifyResult {
  verified: boolean
  txid: string
  amount: string
  confirmations: number
}

export interface NapiShieldedVerifyResult {
  verified: boolean
  txid: string
  observedAmountZat: string
  memoMatched: boolean
  outputsDecrypted: number
}

export interface NapiWalletBalance {
  spendableZat: string
  pendingZat: string
  totalZat: string
}

export declare class ZimppyWalletNapi {
  static open(dataDir: string, lwdEndpoint: string, network: string, seedPhrase: string | null, birthdayHeight: number | null): Promise<ZimppyWalletNapi>
  sync(): Promise<boolean>
  address(): Promise<string>
  balance(): Promise<NapiWalletBalance>
  send(to: string, amountZat: string, memo?: string | null): Promise<string>
  rescan(): Promise<void>
  seedPhrase(): Promise<string | null>
  orchardIvk(): Promise<string>
  network(): string
}
