#!/usr/bin/env npx tsx
/**
 * zimppy — Private machine payments on Zcash
 *
 * A curl-compatible CLI for discovering services and calling HTTP endpoints
 * with automatic shielded Zcash payment handling.
 *
 * Commands:
 *   zimppy wallet login          Set up or configure wallet
 *   zimppy wallet whoami         Show wallet address, balance, network
 *   zimppy wallet balance        Show balance
 *   zimppy wallet fund           Instructions to fund the wallet
 *   zimppy wallet services       Discover paid services (--search <query>)
 *   zimppy request               Make HTTP request with auto-pay
 *   zimppy --help                Show help
 *   zimppy --version             Show version
 */

import { execFileSync, execSync } from 'node:child_process'
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'

const VERSION = '0.1.0'
const CONFIG_DIR = process.env.ZIMPPY_HOME ?? join(homedir(), '.zimppy')
const CONFIG_FILE = join(CONFIG_DIR, 'config.json')
const SESSION_FILE = join(CONFIG_DIR, 'session.json')
const DEFAULT_LWD = 'testnet.zec.rocks:443'
const DEFAULT_RPC = 'https://zcash-testnet-zebrad.gateway.tatum.io'
const DEFAULT_DEPOSIT = '100000' // 100,000 zat default session deposit

// ── Config ──────────────────────────────────────────────────────────

interface ZimppyConfig {
  walletDir: string
  identityFile: string
  lwdServer: string
  rpcEndpoint: string
  network: 'testnet' | 'mainnet'
}

function loadConfig(): ZimppyConfig | null {
  if (!existsSync(CONFIG_FILE)) return null
  return JSON.parse(readFileSync(CONFIG_FILE, 'utf-8')) as ZimppyConfig
}

function saveConfig(config: ZimppyConfig): void {
  mkdirSync(CONFIG_DIR, { recursive: true })
  writeFileSync(CONFIG_FILE, JSON.stringify(config, null, 2))
}

function requireConfig(): ZimppyConfig {
  const cfg = loadConfig()
  if (!cfg) {
    console.error('No wallet configured. Run: zimppy wallet login')
    process.exit(1)
  }
  return cfg
}

// ── Session state ───────────────────────────────────────────────────

interface SessionState {
  sessionId: string
  bearer: string  // the deposit txid
  url: string     // base URL of the session server
}

function loadSession(): SessionState | null {
  if (!existsSync(SESSION_FILE)) return null
  try { return JSON.parse(readFileSync(SESSION_FILE, 'utf-8')) as SessionState }
  catch { return null }
}

function saveSession(session: SessionState): void {
  mkdirSync(CONFIG_DIR, { recursive: true })
  writeFileSync(SESSION_FILE, JSON.stringify(session, null, 2))
}

function clearSession(): void {
  if (existsSync(SESSION_FILE)) {
    writeFileSync(SESSION_FILE, '')
  }
}

// ── Wallet commands ─────────────────────────────────────────────────

async function walletLogin(): Promise<void> {
  console.error('Setting up Zcash wallet for zimppy...')

  // Check if zcash-devtool is available
  try {
    execFileSync('zcash-devtool', ['--help'], { stdio: 'pipe' })
  } catch {
    console.error('ERROR: zcash-devtool not found.')
    console.error('Install: cargo install --git https://github.com/zcash/zcash-devtool')
    process.exit(1)
  }

  const walletDir = process.env.ZCASH_WALLET_DIR ?? join(CONFIG_DIR, 'wallet')
  const identityFile = join(walletDir, 'identity.txt')

  // Check if wallet already exists
  if (existsSync(walletDir) && existsSync(identityFile)) {
    console.error(`Wallet already exists at ${walletDir}`)
    const config: ZimppyConfig = {
      walletDir,
      identityFile,
      lwdServer: DEFAULT_LWD,
      rpcEndpoint: DEFAULT_RPC,
      network: 'testnet',
    }
    saveConfig(config)
    console.error('Config saved.')
    await walletWhoami()
    return
  }

  // Point to existing wallet if env var set
  if (process.env.ZCASH_WALLET_DIR) {
    console.error(`Using existing wallet at ${walletDir}`)
    const config: ZimppyConfig = {
      walletDir,
      identityFile,
      lwdServer: process.env.ZCASH_LWD_SERVER ?? DEFAULT_LWD,
      rpcEndpoint: process.env.ZCASH_RPC_ENDPOINT ?? DEFAULT_RPC,
      network: 'testnet',
    }
    saveConfig(config)
    console.error('Config saved.')
    await walletWhoami()
    return
  }

  console.error(`Creating new wallet at ${walletDir}...`)
  console.error('This requires interactive input for the mnemonic phrase.')
  console.error('Run zcash-devtool manually:')
  console.error(`  mkdir -p ${walletDir}`)
  console.error(`  age-keygen > ${identityFile}`)
  console.error(`  zcash-devtool wallet -w ${walletDir} init --name zimppy -i ${identityFile} --network test --server ${DEFAULT_LWD} --connection direct --birthday 3906900`)
  console.error('')
  console.error('Then run: zimppy wallet login')
}

async function walletWhoami(): Promise<void> {
  const cfg = requireConfig()
  console.error('Syncing wallet...')

  try {
    execFileSync('zcash-devtool', [
      'wallet', '-w', cfg.walletDir, 'sync',
      '--server', cfg.lwdServer, '--connection', 'direct',
    ], { stdio: 'pipe' })
  } catch {
    console.error('Sync failed — lightwalletd may be unavailable')
  }

  try {
    const addr = execFileSync('zcash-devtool', [
      'wallet', '-w', cfg.walletDir, 'list-addresses',
    ], { stdio: 'pipe' }).toString().trim()

    const bal = execFileSync('zcash-devtool', [
      'wallet', '-w', cfg.walletDir, 'balance',
    ], { stdio: 'pipe' }).toString().trim()

    const addrLine = addr.split('\n').find((l: string) => l.includes('utest1') || l.includes('u1'))?.trim() ?? addr
    const address = addrLine.replace(/^.*?Address:\s*/, '').trim()
    const shortAddr = address.length > 50 ? `${address.slice(0, 25)}...${address.slice(-15)}` : address

    const lines = bal.split('\n').map((l: string) => l.trim()).filter(Boolean)
    const total = lines.find((l: string) => l.startsWith('Balance:'))?.replace('Balance:', '').trim() ?? '?'
    const orchard = lines.find((l: string) => l.includes('Orchard Spendable'))?.split(':')[1]?.trim() ?? '0'
    const height = lines.find((l: string) => l.includes('Height:'))?.split(':')[1]?.trim() ?? '?'

    console.log(`--- Zimppy Wallet ---`)
    console.log(`  Address:  ${shortAddr}`)
    console.log(`  Balance:  ${total}`)
    console.log(`  Orchard:  ${orchard}`)
    console.log(`  Network:  ${cfg.network}`)
    console.log(`  Height:   ${height}`)
    console.log(`  Status:   Ready`)
    console.log(`---`)
  } catch (e) {
    console.error(`ERROR: ${(e as Error).message}`)
  }
}

async function walletBalance(): Promise<void> {
  const cfg = requireConfig()
  try {
    process.stderr.write('Syncing...')
    execFileSync('zcash-devtool', [
      'wallet', '-w', cfg.walletDir, 'sync',
      '--server', cfg.lwdServer, '--connection', 'direct',
    ], { stdio: 'pipe' })
    process.stderr.write(' done\n')
    const bal = execFileSync('zcash-devtool', [
      'wallet', '-w', cfg.walletDir, 'balance',
    ], { stdio: 'pipe' }).toString().trim()

    // Parse balance lines
    const lines = bal.split('\n').map((l: string) => l.trim()).filter(Boolean)
    const total = lines.find((l: string) => l.startsWith('Balance:'))?.replace('Balance:', '').trim() ?? '?'
    const orchard = lines.find((l: string) => l.includes('Orchard Spendable'))?.split(':')[1]?.trim() ?? '0'
    const sapling = lines.find((l: string) => l.includes('Sapling Spendable'))?.split(':')[1]?.trim() ?? '0'
    const transparent = lines.find((l: string) => l.includes('Unshielded Spendable'))?.split(':')[1]?.trim() ?? '0'

    console.log(`--- Wallet Balance ---`)
    console.log(`  Total:       ${total}`)
    console.log(`  Orchard:     ${orchard}`)
    console.log(`  Sapling:     ${sapling}`)
    console.log(`  Transparent: ${transparent}`)
    console.log(`  Network:     ${cfg.network}`)
    console.log(`---`)
  } catch (e) {
    console.error(`ERROR: ${(e as Error).message}`)
  }
}

async function walletFund(): Promise<void> {
  const cfg = requireConfig()
  console.log('To fund your zimppy wallet:')
  console.log('')
  console.log('1. Visit https://testnet.zecfaucet.com/')
  console.log('2. Or ask someone to send testnet ZEC to your address')
  console.log('3. Run: zimppy wallet whoami   (to see your address)')
  console.log('')
  console.log(`Wallet: ${cfg.walletDir}`)
  console.log(`Network: ${cfg.network}`)
}

async function walletServices(args: string[]): Promise<void> {
  let searchQuery = ''
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--search' && args[i + 1]) searchQuery = args[++i]
    else if (!args[i].startsWith('-')) searchQuery = args[i]
  }

  // For now, show our known services
  // In production, this would query a registry
  const services = [
    {
      id: 'zimppy-fortune',
      name: 'Zimppy Fortune Teller',
      url: 'http://localhost:3180',
      endpoints: [
        { path: '/api/fortune', method: 'GET', price: '42000 zat', description: 'Get a privacy fortune' },
        { path: '/api/session/fortune', method: 'GET', price: '5000 zat/request (session)', description: 'Fortune via prepaid session' },
        { path: '/api/stream/fortune', method: 'GET', price: '1000 zat/word (stream)', description: 'Streamed fortune, pay per word' },
      ],
      discovery: 'http://localhost:3180/.well-known/payment',
    },
  ]

  if (searchQuery) {
    const filtered = services.filter(s =>
      s.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      s.id.toLowerCase().includes(searchQuery.toLowerCase())
    )
    console.log(JSON.stringify(filtered, null, 2))
  } else {
    console.log(JSON.stringify(services, null, 2))
  }
}

// ── Request command ─────────────────────────────────────────────────

async function handleRequest(args: string[]): Promise<void> {
  const cfg = requireConfig()

  // Parse curl-like args
  let method = 'GET'
  let body: string | undefined
  let url = ''
  let dryRun = false
  let depositOverride: number | undefined
  const headers: Record<string, string> = {}

  for (let i = 0; i < args.length; i++) {
    if (args[i] === '-X' && args[i + 1]) method = args[++i]
    else if (args[i] === '--json' && args[i + 1]) { body = args[++i]; method = method === 'GET' ? 'POST' : method }
    else if (args[i] === '-H' && args[i + 1]) { const [k, ...v] = args[++i].split(':'); headers[k.trim()] = v.join(':').trim() }
    else if (args[i] === '--dry-run') dryRun = true
    else if (args[i] === '--deposit' && args[i + 1]) { depositOverride = Number(args[++i]) }
    else if (args[i] === '-m' && args[i + 1]) { i++ } // timeout, ignore for now
    else if (!args[i].startsWith('-')) url = args[i]
  }

  if (!url) {
    console.error('Usage: zimppy request [--dry-run] [--deposit <zat>] [-X METHOD] [--json \'...\'] <URL>')
    process.exit(1)
  }

  if (body) headers['content-type'] = 'application/json'

  if (dryRun) {
    console.error(`[dry-run] ${method} ${url}`)
    if (body) console.error(`[dry-run] Body: ${body}`)
    return
  }

  console.error(`→ ${method} ${url}`)

  // Check if we have an active session for this URL's base
  const baseUrl = new URL(url).origin
  const session = loadSession()
  if (session && session.url === baseUrl) {
    console.error(`  🎫 Using active session: ${session.sessionId}`)
    const bearerCred = Buffer.from(JSON.stringify({
      payload: { action: 'bearer', sessionId: session.sessionId, bearer: session.bearer },
    }), 'utf-8').toString('base64url')

    const sessionResp = await fetch(url, {
      method,
      headers: { ...headers, Authorization: `Payment ${bearerCred}` },
      body,
    })

    if (sessionResp.status === 200) {
      console.error(`  ✅ Session bearer accepted`)
      console.log(await sessionResp.text())
      return
    }
    // Session might be expired/closed — fall through to regular flow
    console.error(`  ⚠️  Session bearer rejected (${sessionResp.status}), falling back to new payment`)
    clearSession()
  }

  // First request
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
    console.error('ERROR: No payment challenge found')
    process.exit(1)
  }

  const padded = requestMatch[1] + '=='.slice(0, (4 - (requestMatch[1].length % 4)) % 4)
  const challenge = JSON.parse(Buffer.from(padded, 'base64url').toString('utf-8')) as {
    challengeId: string; amount: string; recipient: string; memo: string
  }

  // For session endpoints, deposit more than a single request's worth
  const isSessionEndpoint = url.includes('/session/') || url.includes('/stream/')
  const perRequestZat = Number(challenge.amount)
  const depositMultiplier = 10
  const sendAmountZat = isSessionEndpoint
    ? (depositOverride ?? perRequestZat * depositMultiplier)
    : perRequestZat
  const sendAmount = String(sendAmountZat)
  const amountZec = (sendAmountZat / 100_000_000).toFixed(8)
  console.error('')
  console.error(`  --- Payment Challenge ---`)
  if (isSessionEndpoint) {
    console.error(`  Per-req:   ${challenge.amount} zat`)
    console.error(`  Deposit:   ${sendAmount} zat (${amountZec} ZEC) [${depositOverride ? 'custom' : `${depositMultiplier}x`}]`)
  } else {
    console.error(`  Amount:    ${sendAmount} zat (${amountZec} ZEC)`)
  }
  console.error(`  Recipient: ${challenge.recipient.slice(0, 40)}...`)
  console.error(`  Memo:      ${challenge.memo.slice(0, 40)}...`)
  console.error(`  ---`)
  console.error('')
  console.error('  🔒 Sending shielded Zcash payment...')

  // Sync wallet
  process.stderr.write('  ⏳ Syncing wallet...')
  try {
    execFileSync('zcash-devtool', [
      'wallet', '-w', cfg.walletDir, 'sync',
      '--server', cfg.lwdServer, '--connection', 'direct',
    ], { stdio: 'pipe' })
    console.error(' done')
  } catch {
    console.error(' (skipped)')
  }

  // Send payment
  process.stderr.write('  📡 Broadcasting transaction...')
  let txid: string
  try {
    const out = execFileSync('zcash-devtool', [
      'wallet', '-w', cfg.walletDir, 'send',
      '-i', cfg.identityFile,
      '--server', cfg.lwdServer, '--connection', 'direct',
      '--address', challenge.recipient,
      '--value', sendAmount,
      '--memo', challenge.memo,
    ], { stdio: 'pipe' }).toString()

    txid = out.split('\n').find(l => l.length === 64 && /^[a-f0-9]+$/.test(l)) ?? ''
    if (!txid) {
      console.error(' FAILED')
      console.error('ERROR: No txid in send output')
      console.error(out)
      process.exit(1)
    }
  } catch (e) {
    console.error(' FAILED')
    console.error(`ERROR: Send failed: ${(e as Error).message}`)
    process.exit(1)
  }

  console.error(' done')
  console.error(`  📦 txid: ${txid.slice(0, 16)}...${txid.slice(-8)}`)
  console.error('')

  // Wait for confirmation with progress
  console.error('  ⛏️  Waiting for Zcash block confirmation (~75s)...')
  const startTime = Date.now()
  for (let i = 0; i < 20; i++) {
    await new Promise(r => setTimeout(r, 15000))
    const elapsed = Math.round((Date.now() - startTime) / 1000)
    const bar = '█'.repeat(Math.min(i + 1, 10)) + '░'.repeat(Math.max(10 - i - 1, 0))
    process.stderr.write(`\r  ⛏️  [${bar}] ${elapsed}s elapsed...`)
    try {
      const resp = await fetch(cfg.rpcEndpoint, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ jsonrpc: '2.0', method: 'getrawtransaction', params: [txid, 1], id: 1 }),
      })
      const data = await resp.json() as { result?: { confirmations?: number } }
      if (data.result?.confirmations && data.result.confirmations > 0) {
        const totalTime = Math.round((Date.now() - startTime) / 1000)
        console.error(`\r  ✅ Confirmed in ${totalTime}s (${data.result.confirmations} confirmations)          `)
        break
      }
    } catch { /* keep polling */ }
  }

  if (isSessionEndpoint) {
    // Open a session with the deposit txid
    console.error('  🎫 Opening session with deposit...')
    const openCred = Buffer.from(JSON.stringify({
      payload: { action: 'open', depositTxid: txid, refundAddress: cfg.walletDir },
    }), 'utf-8').toString('base64url')

    const openResp = await fetch(url, {
      method,
      headers: { ...headers, Authorization: `Payment ${openCred}` },
      body,
    })
    const openResult = await openResp.json() as { sessionId?: string; status?: string; fortune?: string }

    if (openResp.status === 200 && openResult.sessionId) {
      saveSession({ sessionId: openResult.sessionId, bearer: txid, url: baseUrl })
      console.error(`  ✅ Session opened: ${openResult.sessionId}`)
      console.error('')
      console.error(`  --- Session Active ---`)
      console.error(`  Session:  ${openResult.sessionId}`)
      console.error(`  Deposit:  ${sendAmount} zat (${amountZec} ZEC)`)
      console.error(`  Privacy:  🔒 Fully shielded (Orchard)`)
      console.error(`  Next:     Bearer requests are instant!`)
      console.error(`  ---`)
      console.error('')
      // If the open also returned content (fortune), show it
      if (openResult.fortune) {
        console.log(JSON.stringify(openResult))
      } else {
        // Make a bearer request to get actual content
        console.error('  🎫 Fetching content via bearer...')
        const bearerCred = Buffer.from(JSON.stringify({
          payload: { action: 'bearer', sessionId: openResult.sessionId, bearer: txid },
        }), 'utf-8').toString('base64url')
        const contentResp = await fetch(url, {
          method,
          headers: { ...headers, Authorization: `Payment ${bearerCred}` },
          body,
        })
        console.log(await contentResp.text())
      }
    } else {
      console.error(`  ❌ Session open failed: ${JSON.stringify(openResult)}`)
    }
  } else {
    // Regular charge flow — retry with credential
    console.error('  🔄 Retrying with payment credential...')

    const credential = Buffer.from(JSON.stringify({
      payload: { txid, challengeId: challenge.challengeId },
    }), 'utf-8').toString('base64url')

    const resp2 = await fetch(url, {
      method,
      headers: { ...headers, Authorization: `Payment ${credential}` },
      body,
    })

    const result = await resp2.text()

    if (resp2.status === 200) {
      console.error('')
      console.error(`  --- Payment Complete ---`)
      console.error(`  ✅ Status:  Verified`)
      console.error(`  💰 Paid:    ${sendAmount} zat (${amountZec} ZEC)`)
      console.error(`  📦 txid:    ${txid.slice(0, 16)}...${txid.slice(-8)}`)
      console.error(`  🔒 Privacy: Fully shielded (Orchard)`)
      console.error(`  ---`)
      console.error('')
      console.log(result)
    } else {
      console.error(`  ❌ ${resp2.status}: Payment verification failed`)
      console.log(result)
    }
  }
}

// ── Session commands ────────────────────────────────────────────────

async function handleSessionClose(): Promise<void> {
  const session = loadSession()
  if (!session) {
    console.error('No active session.')
    return
  }

  console.error(`Closing session ${session.sessionId}...`)
  const closeCred = Buffer.from(JSON.stringify({
    payload: { action: 'close', sessionId: session.sessionId, bearer: session.bearer },
  }), 'utf-8').toString('base64url')

  // Use any session endpoint URL to send the close
  const closeUrl = `${session.url}/api/session/fortune`
  const resp = await fetch(closeUrl, {
    headers: { Authorization: `Payment ${closeCred}` },
  })

  const resultText = await resp.text()
  clearSession()

  if (resp.status === 200) {
    // Check server logs via receipt header for refund info
    const receiptHeader = resp.headers.get('payment-receipt') ?? ''
    let refundInfo = 'none (fully spent)'
    try {
      const padded = receiptHeader + '=='.slice(0, (4 - (receiptHeader.length % 4)) % 4)
      const receipt = JSON.parse(Buffer.from(padded, 'base64url').toString('utf-8')) as Record<string, unknown>
      if (receipt.refundTxid) {
        refundInfo = `${(receipt.refundTxid as string).slice(0, 16)}...`
      }
    } catch { /* no receipt */ }

    // Also try parsing the JSON body for session close details
    try {
      const body = JSON.parse(resultText) as Record<string, unknown>
      if (body.refundTxid) refundInfo = `${(body.refundTxid as string).slice(0, 16)}...`
    } catch { /* not json */ }

    console.log(`--- Session Closed ---`)
    console.log(`  Session:  ${session.sessionId}`)
    console.log(`  Refund:   ${refundInfo}`)
    console.log(`  Status:   Closed`)
    console.log(`---`)
  } else {
    console.error(`Close failed: ${resultText}`)
  }
}

async function handleSessionStatus(): Promise<void> {
  const session = loadSession()
  if (!session) {
    console.log('No active session.')
    return
  }
  console.log(`--- Active Session ---`)
  console.log(`  Session:  ${session.sessionId}`)
  console.log(`  Server:   ${session.url}`)
  console.log(`  Bearer:   ${session.bearer.slice(0, 16)}...${session.bearer.slice(-8)}`)
  console.log(`---`)
}

// ── Main dispatch ───────────────────────────────────────────────────

async function main(): Promise<void> {
  const args = process.argv.slice(2)
  const cmd = args[0]

  if (!cmd || cmd === '--help' || cmd === 'help') {
    console.log(`zimppy v${VERSION} — Private machine payments on Zcash

Commands:
  zimppy wallet login            Set up wallet
  zimppy wallet whoami           Show address + balance + network
  zimppy wallet balance          Show balance
  zimppy wallet fund             How to add funds
  zimppy wallet services         List available paid services
  zimppy wallet services --search <query>  Search services
  zimppy request <URL>           Make HTTP request with auto-pay
  zimppy request -t <URL>        Terse output (agent-friendly)
  zimppy request --dry-run <URL> Show what would be sent
  zimppy --version               Show version

Request options:
  -X METHOD        HTTP method (default: GET)
  --json '{...}'   Send JSON body (implies POST)
  -H 'Key: Val'    Add header
  -t               Terse/compact output for agents
  --dry-run        Show request without sending
  -m <seconds>     Timeout

Examples:
  zimppy wallet whoami
  zimppy wallet services --search fortune
  zimppy request http://localhost:3180/api/fortune
  zimppy request -t -X GET http://localhost:3180/api/fortune`)
    return
  }

  if (cmd === '--version' || cmd === '-v') {
    console.log(VERSION)
    return
  }

  if (cmd === 'wallet') {
    const sub = args[1]
    switch (sub) {
      case 'login': return walletLogin()
      case 'whoami': return walletWhoami()
      case 'balance': return walletBalance()
      case 'fund': return walletFund()
      case 'services': return walletServices(args.slice(2))
      default:
        console.error(`Unknown wallet command: ${sub}`)
        console.error('Available: login, whoami, balance, fund, services')
        process.exit(1)
    }
  }

  if (cmd === 'request') {
    return handleRequest(args.slice(1))
  }

  if (cmd === 'session') {
    const sub = args[1]
    if (sub === 'close') return handleSessionClose()
    if (sub === 'status') return handleSessionStatus()
    console.error('Usage: zimppy session [close|status]')
    process.exit(1)
  }

  console.error(`Unknown command: ${cmd}. Run: zimppy --help`)
  process.exit(1)
}

main().catch((e) => {
  console.error(`ERROR: ${(e as Error).message}`)
  process.exit(1)
})
