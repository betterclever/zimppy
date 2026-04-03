export interface VerifyResult {
  verified: boolean
  txid: string
  observedAddress: string
  observedAmountZat: string
  confirmations: number
}

export interface ShieldedVerifyResult {
  verified: boolean
  txid: string
  observedAmountZat: string
  memoMatched: boolean
  outputsDecrypted: number
}

export interface ZimppyWalletBalance {
  spendableZat: string
  pendingZat: string
  totalZat: string
}

export class ZimppyCore {
  constructor(rpcEndpoint: string)
  verifyTransparent(
    txid: string,
    outputIndex: number,
    expectedAddress: string,
    expectedAmountZat: string,
  ): Promise<VerifyResult>
  verifyShielded(
    txid: string,
    orchardIvk: string,
    expectedChallengeId: string,
    expectedAmountZat: string,
  ): Promise<ShieldedVerifyResult>
  health(): Promise<string>
}

export class ZimppyWalletNapi {
  static open(
    dataDir: string,
    lwdEndpoint: string,
    network: string,
    passphrase?: string | null,
  ): Promise<ZimppyWalletNapi>
  static create(
    dataDir: string,
    lwdEndpoint: string,
    network: string,
    birthdayHeight?: number | null,
    passphrase?: string | null,
  ): Promise<ZimppyWalletNapi>
  static restore(
    dataDir: string,
    lwdEndpoint: string,
    network: string,
    seedPhrase: string,
    birthdayHeight: number,
    passphrase?: string | null,
  ): Promise<ZimppyWalletNapi>
  sync(): Promise<boolean>
  ensureReady(): Promise<boolean>
  address(): Promise<string>
  balance(): Promise<ZimppyWalletBalance>
  send(to: string, amountZat: string, memo?: string | null): Promise<string>
  seedPhrase(): Promise<string | null>
  fullAddress(): Promise<string>
  rescan(): Promise<void>
  orchardIvk(): Promise<string>
  setMinConfirmations(minConf: number): Promise<void>
  close(): Promise<void>
  network(): string
}
