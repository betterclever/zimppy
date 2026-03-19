import { Challenge, Credential, Receipt } from 'mppx'
import { zcashClient, zcashMethod } from 'zimppy-ts'
import {
  createLogger,
  LWD_SERVER,
  RPC_ENDPOINT,
  sendRealPayment,
  WALLET_DIR,
} from './autopay.js'

const SERVER_URL = process.env.SERVER_URL ?? 'http://127.0.0.1:3180'

const log = createLogger()

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
      return await sendRealPayment(request, log)
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

main().catch((error: unknown) => {
  console.error(error instanceof Error ? error.stack ?? error.message : String(error))
  process.exitCode = 1
})
