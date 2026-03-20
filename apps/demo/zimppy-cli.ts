#!/usr/bin/env npx tsx
/**
 * zimppy CLI — curl-compatible HTTP client with automatic Zcash payments.
 *
 * Like Tempo's CLI but for Zcash. Makes paid HTTP requests with automatic
 * shielded payment handling.
 *
 * Usage:
 *   zimppy request <URL>                    # GET with auto-pay
 *   zimppy request -X POST --json '{}' <URL> # POST with auto-pay
 *   zimppy wallet whoami                     # Show wallet info
 *   zimppy wallet balance                    # Show balance
 *   zimppy discover <URL>                    # Check /.well-known/payment
 */

import { sendRealPayment, syncWallet, WALLET_DIR, LWD_SERVER } from './autopay.js'
import { execFile } from 'node:child_process'
import { promisify } from 'node:util'

const execFileAsync = promisify(execFile)

const command = process.argv[2]
const args = process.argv.slice(3)

async function main() {
  switch (command) {
    case 'request':
      await handleRequest(args)
      break
    case 'wallet':
      await handleWallet(args)
      break
    case 'discover':
      await handleDiscover(args)
      break
    case 'help':
    case undefined:
      printHelp()
      break
    default:
      console.error(`Unknown command: ${command}`)
      printHelp()
      process.exit(1)
  }
}

async function handleRequest(args: string[]) {
  // Parse args (simplified curl-like)
  let method = 'GET'
  let body: string | undefined
  let url = ''

  for (let i = 0; i < args.length; i++) {
    if (args[i] === '-X' && args[i + 1]) {
      method = args[++i]
    } else if (args[i] === '--json' && args[i + 1]) {
      body = args[++i]
      method = method === 'GET' ? 'POST' : method
    } else if (!args[i].startsWith('-')) {
      url = args[i]
    }
  }

  if (!url) {
    console.error('Usage: zimppy request <URL>')
    process.exit(1)
  }

  console.error(`→ ${method} ${url}`)

  const headers: Record<string, string> = {}
  if (body) headers['content-type'] = 'application/json'

  const resp1 = await fetch(url, { method, headers, body })

  if (resp1.status !== 402) {
    console.error(`← ${resp1.status}`)
    console.log(await resp1.text())
    return
  }

  console.error('← 402 Payment Required')

  // Parse challenge
  const wwwAuth = resp1.headers.get('www-authenticate') ?? ''
  const requestMatch = wwwAuth.match(/request="([^"]+)"/)
  if (!requestMatch) {
    console.error('ERROR: No payment challenge in response')
    process.exit(1)
  }

  const padded = requestMatch[1] + '=='.slice(0, (4 - (requestMatch[1].length % 4)) % 4)
  const challenge = JSON.parse(Buffer.from(padded, 'base64url').toString('utf-8')) as {
    challengeId: string
    amount: string
    recipient: string
    memo: string
  }

  console.error(`  Amount: ${challenge.amount} zat`)
  console.error(`  To: ${challenge.recipient.slice(0, 30)}...`)
  console.error(`  Memo: ${challenge.memo}`)
  console.error('')
  console.error('→ Paying with Zcash...')

  const payment = await sendRealPayment(challenge, (line) => {
    if (line) console.error(`  ${line}`)
  })

  console.error('')
  console.error('→ Retrying with credential...')

  const credential = Buffer.from(
    JSON.stringify({ payload: { txid: payment.txid, challengeId: payment.challengeId } }),
    'utf-8'
  ).toString('base64url')

  const resp2 = await fetch(url, {
    method,
    headers: { ...headers, Authorization: `Payment ${credential}` },
    body,
  })

  console.error(`← ${resp2.status}`)
  const result = await resp2.text()
  console.log(result)

  const receipt = resp2.headers.get('payment-receipt')
  if (receipt) {
    console.error(`  Receipt: ${receipt.slice(0, 80)}...`)
  }
}

async function handleWallet(args: string[]) {
  const sub = args[0]

  switch (sub) {
    case 'whoami': {
      await syncWallet(console.error)
      const { stdout } = await execFileAsync('zcash-devtool', [
        'wallet', '-w', WALLET_DIR, 'list-addresses',
      ])
      console.log(stdout.trim())
      break
    }
    case 'balance': {
      await syncWallet(console.error)
      const { stdout } = await execFileAsync('zcash-devtool', [
        'wallet', '-w', WALLET_DIR, 'balance',
      ])
      console.log(stdout.trim())
      break
    }
    case 'sync': {
      await syncWallet(console.log)
      console.log('Wallet synced.')
      break
    }
    default:
      console.log('Usage: zimppy wallet [whoami|balance|sync]')
  }
}

async function handleDiscover(args: string[]) {
  const baseUrl = args[0]
  if (!baseUrl) {
    console.error('Usage: zimppy discover <BASE_URL>')
    process.exit(1)
  }

  const url = baseUrl.replace(/\/$/, '') + '/.well-known/payment'
  console.error(`→ GET ${url}`)

  try {
    const resp = await fetch(url)
    if (resp.ok) {
      const data = await resp.json()
      console.log(JSON.stringify(data, null, 2))
    } else {
      console.error(`← ${resp.status} — no payment discovery at this endpoint`)
    }
  } catch (err) {
    console.error(`ERROR: ${(err as Error).message}`)
  }
}

function printHelp() {
  console.log(`
zimppy — Private machine payments on Zcash

Commands:
  request <URL>          Make an HTTP request with auto-pay
  wallet whoami          Show wallet address
  wallet balance         Show wallet balance
  wallet sync            Sync wallet with chain
  discover <BASE_URL>    Check service payment info

Options for request:
  -X METHOD              HTTP method (default: GET)
  --json '{"key":"val"}' Send JSON body (implies POST)

Examples:
  zimppy request http://localhost:3180/api/fortune
  zimppy request -X POST --json '{"city":"Tokyo"}' http://localhost:3180/api/weather
  zimppy wallet balance
  zimppy discover http://localhost:3180
`.trim())
}

main().catch((err) => {
  console.error(`ERROR: ${(err as Error).message}`)
  process.exit(1)
})
