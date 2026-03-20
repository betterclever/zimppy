#!/usr/bin/env npx tsx
/**
 * zimppy-pay CLI — make paid HTTP requests with automatic Zcash payment.
 *
 * Usage: npx tsx apps/demo/autopay-cli.ts <URL>
 *
 * If the server returns 402, this tool:
 * 1. Parses the WWW-Authenticate challenge
 * 2. Sends real ZEC via zcash-devtool
 * 3. Waits for confirmation
 * 4. Retries with the credential
 * 5. Prints the response
 */

import { sendRealPayment } from './autopay.js'

const url = process.argv[2]
if (!url) {
  console.error('Usage: npx tsx apps/demo/autopay-cli.ts <URL>')
  process.exit(1)
}

async function main() {
  console.log(`→ GET ${url}`)

  // First request
  const resp1 = await fetch(url)

  if (resp1.status !== 402) {
    console.log(`← ${resp1.status}`)
    console.log(await resp1.text())
    return
  }

  console.log('← 402 Payment Required')

  // Parse challenge from WWW-Authenticate header
  const wwwAuth = resp1.headers.get('www-authenticate') ?? ''
  const requestMatch = wwwAuth.match(/request="([^"]+)"/)
  if (!requestMatch) {
    console.error('ERROR: No request in WWW-Authenticate header')
    process.exit(1)
  }

  const challengeJson = JSON.parse(
    Buffer.from(requestMatch[1] + '==', 'base64url').toString('utf-8')
  ) as {
    challengeId: string
    amount: string
    recipient: string
    memo: string
  }

  console.log(`  Challenge: send ${challengeJson.amount} zat to ${challengeJson.recipient.slice(0, 25)}...`)
  console.log(`  Memo: ${challengeJson.memo}`)
  console.log('')

  // Pay
  const payment = await sendRealPayment(challengeJson, (line) => {
    if (line) console.log(`  ${line}`)
  })

  console.log('')
  console.log(`→ Retrying with credential...`)

  // Retry with credential
  const credential = Buffer.from(
    JSON.stringify({ payload: { txid: payment.txid, challengeId: payment.challengeId } }),
    'utf-8'
  ).toString('base64url')

  const resp2 = await fetch(url, {
    headers: { Authorization: `Payment ${credential}` },
  })

  console.log(`← ${resp2.status}`)
  const body = await resp2.text()
  console.log(body)

  const receipt = resp2.headers.get('payment-receipt')
  if (receipt) {
    try {
      const decoded = JSON.parse(Buffer.from(receipt + '==', 'base64url').toString('utf-8'))
      console.log(`  Receipt: method=${decoded.method}, reference=${decoded.reference?.slice(0, 20)}...`)
    } catch {
      console.log(`  Receipt: ${receipt.slice(0, 60)}...`)
    }
  }
}

main().catch((err) => {
  console.error(`ERROR: ${(err as Error).message}`)
  process.exit(1)
})
