import { execFile } from 'node:child_process'
import { promisify } from 'node:util'
import { Challenge, Credential, Receipt } from 'mppx'
import { zcashClient, zcashMethod } from 'zimppy-ts'

const execFileAsync = promisify(execFile)

const SERVER_URL = process.env.SERVER_URL ?? 'http://127.0.0.1:3180'
const WALLET_DIR = process.env.ZCASH_WALLET_DIR ?? '/tmp/zcash-wallet-send'
const IDENTITY_FILE = process.env.ZCASH_IDENTITY_FILE ?? '/tmp/zcash-wallet-send/identity.txt'
const LWD_SERVER = process.env.ZCASH_LWD_SERVER ?? 'testnet.zec.rocks:443'
const RPC_ENDPOINT = process.env.ZCASH_RPC_ENDPOINT ?? 'https://zcash-testnet-zebrad.gateway.tatum.io'
const CONFIRMATION_TIMEOUT_MS = Number(process.env.ZCASH_CONFIRMATION_TIMEOUT_MS ?? 5 * 60 * 1000)
const POLL_INTERVAL_MS = Number(process.env.ZCASH_CONFIRMATION_POLL_MS ?? 15_000)

function log(line = ''): void {
  console.log(line)
}

async function main(): Promise<void> {
  log('=== Zcash MPP Auto-Pay Demo ===')
  log(`Server: ${SERVER_URL}`)
  log(`Wallet: ${WALLET_DIR}`)
  log(`Lightwalletd: ${LWD_SERVER}`)
  log(`RPC: ${RPC_ENDPOINT}`)
  log()

  log('Step 1: Requesting protected resource without payment...')
  const initialResponse = await fetch(`${SERVER_URL}/api/fortune`)
  log(`  Status: ${initialResponse.status}`)

  if (initialResponse.status !== 402) {
    const body = await initialResponse.text()
    throw new Error(`expected 402 challenge, got ${initialResponse.status}: ${body}`)
  }

  const challenge = Challenge.fromResponse(initialResponse, { methods: [zcashMethod] })
  log(`  Challenge ID: ${challenge.request.challengeId}`)
  log(`  Memo: ${challenge.request.memo}`)
  log(`  Amount: ${challenge.request.amount} ${challenge.request.currency}`)
  log(`  Recipient: ${challenge.request.recipient}`)
  log()

  const client = zcashClient({
    async createPayment({ challenge: request }) {
      return await sendRealPayment(request)
    },
  })

  log('Step 2: Auto-paying the challenge through zcash-devtool...')
  const authorization = await client.createCredential({ challenge })
  const parsedCredential = Credential.deserialize<{ txid: string; challengeId?: string }>(authorization)
  log(`  Auto-pay txid: ${parsedCredential.payload.txid}`)
  log()

  log('Step 3: Retrying with Payment credential...')
  const paidResponse = await fetch(`${SERVER_URL}/api/fortune`, {
    headers: {
      Authorization: authorization,
    },
  })

  const paidBody = await paidResponse.text()
  log(`  Status: ${paidResponse.status}`)

  if (paidResponse.status !== 200) {
    log(`  Response: ${paidBody}`)
    throw new Error(`paid request failed with status ${paidResponse.status}`)
  }

  const receipt = Receipt.fromResponse(paidResponse)
  const body = JSON.parse(paidBody) as { fortune?: string }
  log(`  Fortune: ${body.fortune ?? '(missing fortune)'}`)
  log(`  Receipt reference: ${receipt.reference}`)
  log(`  Receipt timestamp: ${receipt.timestamp}`)
  log()
  log('=== Auto-pay demo complete ===')
}

async function sendRealPayment(challenge: {
  amount: string
  challengeId: string
  memo: string
  recipient: string
}): Promise<{ txid: string; challengeId: string }> {
  await syncWallet()
  const txid = await broadcastPayment(challenge)
  log(`  Broadcast txid: ${txid}`)
  await waitForConfirmation(txid)
  return {
    txid,
    challengeId: challenge.challengeId,
  }
}

async function syncWallet(): Promise<void> {
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

async function broadcastPayment(challenge: {
  amount: string
  memo: string
  recipient: string
}): Promise<string> {
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

async function waitForConfirmation(txid: string): Promise<void> {
  log('  Waiting for on-chain confirmation...')
  const started = Date.now()

  while (Date.now() - started < CONFIRMATION_TIMEOUT_MS) {
    const confirmations = await fetchConfirmations(txid)
    if (confirmations > 0) {
      log(`  Confirmed with ${confirmations} confirmation(s).`)
      return
    }

    log(`  Still pending. Sleeping ${Math.round(POLL_INTERVAL_MS / 1000)}s...`)
    await sleep(POLL_INTERVAL_MS)
  }

  throw new Error(`confirmation timeout after ${Math.round(CONFIRMATION_TIMEOUT_MS / 1000)} seconds`)
}

async function fetchConfirmations(txid: string): Promise<number> {
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
      return 0
    }
    throw new Error(`rpc error while checking confirmation: ${message}`)
  }

  return payload.result?.confirmations ?? 0
}

function extractTxid(output: string): string | null {
  for (const line of output.split('\n').map((entry) => entry.trim())) {
    if (/^[a-f0-9]{64}$/i.test(line)) {
      return line
    }
  }
  return null
}

function indent(value: string, spaces: number): string {
  const prefix = ' '.repeat(spaces)
  return value
    .split('\n')
    .map((line) => `${prefix}${line}`)
    .join('\n')
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

function isPendingTransactionMessage(message: string): boolean {
  return /no such mempool or main chain transaction/i.test(message)
}

main().catch((error: unknown) => {
  console.error(error instanceof Error ? error.stack ?? error.message : String(error))
  process.exitCode = 1
})
