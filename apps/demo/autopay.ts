import { execFile } from 'node:child_process'
import { writeFileSync } from 'node:fs'
import { promisify } from 'node:util'

const execFileAsync = promisify(execFile)

export const WALLET_DIR = process.env.ZCASH_WALLET_DIR ?? '/tmp/zcash-wallet-send'
export const IDENTITY_FILE = process.env.ZCASH_IDENTITY_FILE ?? '/tmp/zcash-wallet-send/identity.txt'
export const LWD_SERVER = process.env.ZCASH_LWD_SERVER ?? 'testnet.zec.rocks:443'
export const RPC_ENDPOINT = process.env.ZCASH_RPC_ENDPOINT ?? 'https://zcash-testnet-zebrad.gateway.tatum.io'
export const CONFIRMATION_TIMEOUT_MS = Number(process.env.ZCASH_CONFIRMATION_TIMEOUT_MS ?? 5 * 60 * 1000)
export const POLL_INTERVAL_MS = Number(process.env.ZCASH_CONFIRMATION_POLL_MS ?? 15_000)
export const TX_TRACK_FILE = process.env.ZIMPPY_TX_TRACK_FILE ?? '/tmp/zimppy-last-txid'

export type PaymentRequest = {
  amount: string
  challengeId: string
  memo: string
  recipient: string
}

export function createLogger(writer: (line: string) => void = console.log) {
  return (line = ''): void => writer(line)
}

export async function sendRealPayment(
  challenge: PaymentRequest,
  log: (line?: string) => void = console.log,
): Promise<{ txid: string; challengeId: string }> {
  writeTrackedTx('')
  await syncWallet(log)
  const txid = await broadcastPayment(challenge, log)
  writeTrackedTx(txid)
  log(`  Broadcast txid: ${txid}`)
  await waitForConfirmation(txid, log)
  return {
    txid,
    challengeId: challenge.challengeId,
  }
}

export async function syncWallet(log: (line?: string) => void = console.log): Promise<void> {
  log('  Syncing wallet...')
  const { stdout, stderr } = await execFileAsync('zcash-devtool', [
    'wallet',
    '-w',
    WALLET_DIR,
    'sync',
    '--server',
    LWD_SERVER,
    '--connection',
    'direct',
  ])
  const output = [stdout, stderr].filter(Boolean).join('\n').trim()
  if (output) {
    log(indent(output, 4))
  }
}

export async function broadcastPayment(
  challenge: Omit<PaymentRequest, 'challengeId'>,
  log: (line?: string) => void = console.log,
): Promise<string> {
  log(`  Sending ${challenge.amount} zat to ${challenge.recipient.slice(0, 24)}...`)
  log(`  Memo: ${challenge.memo}`)

  const { stdout, stderr } = await execFileAsync('zcash-devtool', [
    'wallet',
    '-w',
    WALLET_DIR,
    'send',
    '-i',
    IDENTITY_FILE,
    '--server',
    LWD_SERVER,
    '--connection',
    'direct',
    '--address',
    challenge.recipient,
    '--value',
    challenge.amount,
    '--memo',
    challenge.memo,
  ])

  const combined = [stdout, stderr].filter(Boolean).join('\n')
  const txid = extractTxid(combined)
  if (!txid) {
    throw new Error(`failed to extract txid from zcash-devtool output:\n${combined}`)
  }
  return txid
}

export async function waitForConfirmation(
  txid: string,
  log: (line?: string) => void = console.log,
): Promise<void> {
  log('  Waiting for on-chain confirmation...')
  const started = Date.now()
  let lastStatus = ''

  while (Date.now() - started < CONFIRMATION_TIMEOUT_MS) {
    const status = await fetchTxStatus(txid)
    const line = status.state === 'confirmed'
      ? `  Status: confirmed (${status.confirmations} confirmation(s))`
      : status.state === 'mempool'
        ? '  Status: seen in mempool (0 confirmations)'
        : '  Status: not yet visible on RPC'

    if (line !== lastStatus) {
      log(line)
      lastStatus = line
    }

    if (status.state === 'confirmed' && (status.confirmations ?? 0) > 0) {
      return
    }

    log(`  Next poll in ${Math.round(POLL_INTERVAL_MS / 1000)}s...`)
    await sleep(POLL_INTERVAL_MS)
  }

  throw new Error(`confirmation timeout after ${Math.round(CONFIRMATION_TIMEOUT_MS / 1000)} seconds`)
}

export async function fetchConfirmations(txid: string): Promise<number> {
  const status = await fetchTxStatus(txid)
  return status.confirmations ?? 0
}

export async function fetchTxStatus(txid: string): Promise<{
  state: 'pending' | 'mempool' | 'confirmed'
  confirmations: number | null
}> {
  const response = await fetch(RPC_ENDPOINT, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      jsonrpc: '2.0',
      id: 1,
      method: 'getrawtransaction',
      params: [txid, 1],
    }),
  })

  const payload = await response.json() as {
    error?: { message?: string }
    result?: { confirmations?: number }
  }

  if (payload.error) {
    const message = payload.error.message ?? 'unknown error'
    if (isPendingTransactionMessage(message)) {
      return { state: 'pending', confirmations: null }
    }
    throw new Error(`rpc error while checking confirmation: ${message}`)
  }

  const confirmations = payload.result?.confirmations ?? 0
  return {
    state: confirmations > 0 ? 'confirmed' : 'mempool',
    confirmations,
  }
}

export function indent(value: string, spaces: number): string {
  const prefix = ' '.repeat(spaces)
  return value
    .split('\n')
    .map((line) => `${prefix}${line}`)
    .join('\n')
}

export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

export function isPendingTransactionMessage(message: string): boolean {
  return /no such mempool or main chain transaction/i.test(message)
}

function extractTxid(output: string): string | null {
  for (const line of output.split('\n').map((entry) => entry.trim())) {
    if (/^[a-f0-9]{64}$/i.test(line)) {
      return line
    }
  }
  return null
}

function writeTrackedTx(txid: string): void {
  writeFileSync(TX_TRACK_FILE, txid, 'utf8')
}
