#!/usr/bin/env tsx
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

import { existsSync, mkdirSync, readFileSync, readdirSync, unlinkSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'
import { createRequire } from 'node:module'
import type { ZimppyWalletNapi } from '@zimppy/core-napi'

const require = createRequire(import.meta.url)
const { Mppx } = require('mppx/client') as typeof import('mppx/client')
const { zcashClient, zcashSessionClient } = require('zimppy-ts') as typeof import('zimppy-ts')

const VERSION = '0.1.0'
const CONFIG_DIR = process.env.ZIMPPY_HOME ?? join(homedir(), '.zimppy')
const WALLETS_DIR = join(CONFIG_DIR, 'wallets')
const CONFIG_FILE = join(CONFIG_DIR, 'config.json')
const SESSION_FILE = join(CONFIG_DIR, 'session.json')
const DEFAULT_LWD = 'https://testnet.zec.rocks'
const DEFAULT_RPC = 'https://zcash-testnet-zebrad.gateway.tatum.io'
const DEFAULT_DEPOSIT = '100000' // 100,000 zat default session deposit

// ── Wallet NAPI ──────────────────────────────────────────────────

async function openWallet(cfg: ZimppyConfig, seedPhrase?: string): Promise<ZimppyWalletNapi> {
  const { ZimppyWalletNapi: Wallet } = require('@zimppy/core-napi') as typeof import('@zimppy/core-napi')
  return Wallet.open(cfg.dataDir, cfg.lwdServer, cfg.network, seedPhrase ?? null, null)
}

function walletDir(name: string): string {
  return join(WALLETS_DIR, name)
}

function activeWalletName(): string {
  try {
    const cfg = JSON.parse(readFileSync(CONFIG_FILE, 'utf-8'))
    return cfg.activeWallet ?? 'default'
  } catch {
    return 'default'
  }
}

// ── Config ──────────────────────────────────────────────────────────

interface ZimppyConfig {
  dataDir: string
  lwdServer: string
  rpcEndpoint: string
  network: 'testnet' | 'mainnet'
  activeWallet?: string
  address?: string  // cached unified address
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
    console.error('No wallet configured. Run: zimppy wallet create')
    process.exit(1)
  }
  return cfg
}

// ── Session state ───────────────────────────────────────────────────

interface SessionState {
  sessionId: string
  bearer: string  // the deposit txid
  url: string     // base URL of the session server
  endpoint?: string // the session endpoint path (e.g. /api/session/fortune)
}

function getWalletAddress(cfg: ZimppyConfig): string {
  if (cfg.address) return cfg.address
  throw new Error('No wallet address cached. Run "zimppy wallet whoami" first.')
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
    unlinkSync(SESSION_FILE)
  }
}

// ── Wallet commands ─────────────────────────────────────────────────

async function walletCreate(args?: string[]): Promise<void> {
  const name = args?.[0] ?? 'default'
  const dataDir = walletDir(name)

  if (existsSync(join(dataDir, 'zingo-wallet.dat'))) {
    console.error(`Wallet '${name}' already exists.`)
    await walletWhoami()
    return
  }

  const config: ZimppyConfig = {
    dataDir,
    lwdServer: DEFAULT_LWD,
    rpcEndpoint: DEFAULT_RPC,
    network: 'testnet',
    activeWallet: name,
  }

  console.error(`Creating wallet '${name}'...`)
  try {
    const wallet = await openWallet(config)
    const addr = await wallet.address()
    const seed = await wallet.seedPhrase()
    console.error('')
    console.error(`  Wallet '${name}' created!`)
    console.error(`  Address: ${addr.slice(0, 25)}...${addr.slice(-15)}`)
    if (seed) {
      console.error('')
      console.error('  === BACKUP YOUR SEED PHRASE ===')
      console.error(`  ${seed}`)
      console.error('  ===============================')
      console.error('  Write this down and store it safely. You need it to recover your wallet.')
    }
  } catch (e) {
    console.error(`ERROR: ${(e as Error).message}`)
    process.exit(1)
  }

  saveConfig(config)
  console.error('')
  console.error('Run `npx zimppy wallet fund` to add testnet ZEC.')
}

async function walletRestore(args: string[]): Promise<void> {
  const phrase = args[0]
  if (!phrase) {
    console.error('Usage: zimppy wallet restore "your 24 word seed phrase" [name] [birthday]')
    process.exit(1)
  }

  const name = args[1] ?? 'default'
  const dataDir = walletDir(name)
  const config: ZimppyConfig = {
    dataDir,
    lwdServer: DEFAULT_LWD,
    rpcEndpoint: DEFAULT_RPC,
    network: 'testnet',
    activeWallet: name,
  }

  console.error(`Restoring wallet '${name}' from seed phrase...`)
  try {
    const wallet = await openWallet(config, phrase)
    const addr = await wallet.address()
    console.error(`Address: ${addr}`)
  } catch (e) {
    console.error(`ERROR: ${(e as Error).message}`)
    process.exit(1)
  }

  saveConfig(config)
  console.error('Config saved.')
  await walletWhoami()
}

async function walletWhoami(): Promise<void> {
  const cfg = requireConfig()

  try {
    const wallet = await openWallet(cfg)

    process.stderr.write('Syncing wallet...')
    await wallet.sync()
    console.error(' done')

    const address = await wallet.address()
    const shortAddr = address.length > 50 ? `${address.slice(0, 25)}...${address.slice(-15)}` : address

    // Cache address in config for refund operations
    if (address && address !== cfg.address) {
      saveConfig({ ...cfg, address })
    }

    const bal = await wallet.balance()

    console.log(`--- Zimppy Wallet ---`)
    console.log(`  Address:  ${shortAddr}`)
    console.log(`  Balance:  ${bal.totalZat} zat`)
    console.log(`  Network:  ${cfg.network}`)
    console.log(`  Status:   Ready`)
    console.log(`---`)
  } catch (e) {
    console.error(`ERROR: ${(e as Error).message}`)
  }
}

async function walletBalance(): Promise<void> {
  const cfg = requireConfig()
  try {
    const wallet = await openWallet(cfg)

    process.stderr.write('Syncing...')
    await wallet.sync()
    process.stderr.write(' done\n')

    const bal = await wallet.balance()

    console.log(`--- Wallet Balance ---`)
    console.log(`  Spendable: ${bal.spendableZat} zat`)
    console.log(`  Pending:   ${bal.pendingZat} zat`)
    console.log(`  Total:     ${bal.totalZat} zat`)
    console.log(`  Network:   ${cfg.network}`)
    console.log(`---`)
  } catch (e) {
    console.error(`ERROR: ${(e as Error).message}`)
  }
}

async function walletList(): Promise<void> {
  if (!existsSync(WALLETS_DIR)) {
    console.error('No wallets found. Run: zimppy wallet create')
    return
  }
  const active = activeWalletName()
  const dirs = readdirSync(WALLETS_DIR).filter(d =>
    existsSync(join(WALLETS_DIR, d, 'zingo-wallet.dat'))
  )
  if (dirs.length === 0) {
    console.error('No wallets found. Run: zimppy wallet create')
    return
  }
  for (const name of dirs) {
    const marker = name === active ? ' (active)' : ''
    console.log(`  ${name}${marker}`)
  }
}

async function walletUse(name?: string): Promise<void> {
  if (!name) {
    console.error('Usage: zimppy wallet use <name>')
    process.exit(1)
  }
  const dataDir = walletDir(name)
  if (!existsSync(join(dataDir, 'zingo-wallet.dat'))) {
    console.error(`Wallet '${name}' does not exist. Run: zimppy wallet create ${name}`)
    process.exit(1)
  }
  const config: ZimppyConfig = {
    dataDir,
    lwdServer: DEFAULT_LWD,
    rpcEndpoint: DEFAULT_RPC,
    network: 'testnet',
    activeWallet: name,
  }
  saveConfig(config)
  console.error(`Switched to wallet '${name}'`)
  await walletWhoami()
}

async function walletSeed(): Promise<void> {
  const cfg = requireConfig()
  try {
    const wallet = await openWallet(cfg)
    const seed = await wallet.seedPhrase()
    if (seed) {
      console.log(seed)
    } else {
      console.error('Seed phrase not available (watch-only wallet)')
    }
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
  console.log(`Wallet: ${cfg.dataDir}`)
  console.log(`Network: ${cfg.network}`)
}

const SERVICES_FILE = join(CONFIG_DIR, 'services.json')
const DEFAULT_SERVICE_URLS = ['http://localhost:3180', 'http://localhost:3181']

function loadServiceUrls(): string[] {
  if (existsSync(SERVICES_FILE)) {
    try { return JSON.parse(readFileSync(SERVICES_FILE, 'utf-8')) as string[] }
    catch { /* fall through */ }
  }
  return DEFAULT_SERVICE_URLS
}

function saveServiceUrls(urls: string[]): void {
  mkdirSync(CONFIG_DIR, { recursive: true })
  writeFileSync(SERVICES_FILE, JSON.stringify(urls, null, 2))
}

async function walletServices(args: string[]): Promise<void> {
  // --add <url>: register a new service
  // --remove <url>: remove a service
  // <url>: discover a specific service
  // (no args): scan all known services
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--add' && args[i + 1]) {
      const urls = loadServiceUrls()
      const newUrl = args[++i].replace(/\/$/, '')
      if (!urls.includes(newUrl)) { urls.push(newUrl); saveServiceUrls(urls) }
      console.error(`✅ Added ${newUrl}`)
      return
    }
    if (args[i] === '--remove' && args[i + 1]) {
      const urls = loadServiceUrls()
      const rmUrl = args[++i].replace(/\/$/, '')
      saveServiceUrls(urls.filter(u => u !== rmUrl))
      console.error(`✅ Removed ${rmUrl}`)
      return
    }
  }

  // Single URL given — discover just that service
  const singleUrl = args.find(a => !a.startsWith('-'))
  if (singleUrl) {
    const base = singleUrl.replace(/\/$/, '')
    console.error(`🔍 ${base}/.well-known/payment`)
    try {
      const resp = await fetch(`${base}/.well-known/payment`, { signal: AbortSignal.timeout(5000) })
      if (!resp.ok) { console.error(`ERROR: ${resp.status}`); process.exit(1) }
      console.log(JSON.stringify(await resp.json(), null, 2))
    } catch (e) { console.error(`ERROR: ${(e as Error).message}`); process.exit(1) }
    return
  }

  // No args — scan all known service URLs
  const urls = loadServiceUrls()
  console.error(`🔍 Scanning ${urls.length} known services...`)

  const services: Record<string, unknown>[] = []
  await Promise.all(urls.map(async (base) => {
    try {
      const resp = await fetch(`${base}/.well-known/payment`, { signal: AbortSignal.timeout(3000) })
      if (resp.ok) {
        const data = await resp.json() as Record<string, unknown>
        services.push({ ...data, url: base })
      }
    } catch { /* service not reachable — skip */ }
  }))

  if (services.length === 0) {
    console.error('No services found. Are any MPP servers running?')
    console.error(`Checked: ${urls.join(', ')}`)
    console.error('Add a service: npx zimppy wallet services --add http://host:port')
    return
  }

  console.error(`Found ${services.length} service(s)`)
  console.log(JSON.stringify(services, null, 2))
}

// ── Helpers ─────────────────────────────────────────────────────────

async function waitForConfirmation(cfg: ZimppyConfig, txid: string): Promise<void> {
  console.error('  Waiting for Zcash block confirmation (~75s)...')
  const startTime = Date.now()
  for (let i = 0; i < 20; i++) {
    await new Promise(r => setTimeout(r, 15000))
    const elapsed = Math.round((Date.now() - startTime) / 1000)
    const bar = '\u2588'.repeat(Math.min(i + 1, 10)) + '\u2591'.repeat(Math.max(10 - i - 1, 0))
    process.stderr.write(`\r  [${bar}] ${elapsed}s elapsed...`)
    try {
      const resp = await fetch(cfg.rpcEndpoint, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ jsonrpc: '2.0', method: 'getrawtransaction', params: [txid, 1], id: 1 }),
      })
      const data = await resp.json() as { result?: { confirmations?: number } }
      if (data.result?.confirmations && data.result.confirmations > 0) {
        const totalTime = Math.round((Date.now() - startTime) / 1000)
        console.error(`\r  Confirmed in ${totalTime}s (${data.result.confirmations} confirmations)          `)
        return
      }
    } catch { /* keep polling */ }
  }
}

async function sendViaWallet(cfg: ZimppyConfig, params: { to: string; amountZat: string; memo: string }): Promise<string> {
  const amountZec = (Number(params.amountZat) / 100_000_000).toFixed(8)
  console.error(`  Sending ${params.amountZat} zat (${amountZec} ZEC)...`)

  const wallet = await openWallet(cfg)
  process.stderr.write('  Syncing wallet...')
  await wallet.sync()
  console.error(' done')

  process.stderr.write('  Broadcasting transaction...')
  const txid = await wallet.send(params.to, params.amountZat, params.memo)
  console.error(' done')
  console.error(`  txid: ${txid.slice(0, 16)}...${txid.slice(-8)}`)
  console.error('')

  await waitForConfirmation(cfg, txid)
  return txid
}

function createMppxClient(cfg: ZimppyConfig) {
  const sessionClient = zcashSessionClient({
    sendPayment: (params) => sendViaWallet(cfg, params),
    refundAddress: cfg.address || (() => {
      console.error('ERROR: No wallet address cached. Run: npx zimppy wallet whoami')
      process.exit(1)
    })(),
  })

  const client = Mppx.create({
    methods: [
      zcashClient({
        createPayment: async ({ challenge }) => {
          console.error('')
          console.error(`  --- Charge ---`)
          console.error(`  Amount:    ${challenge.amount} zat`)
          console.error(`  Recipient: ${challenge.recipient.slice(0, 40)}...`)
          console.error(`  ---`)
          console.error('')

          const txid = await sendViaWallet(cfg, {
            to: challenge.recipient,
            amountZat: challenge.amount,
            memo: challenge.methodDetails?.memo ?? '',
          })

          return { txid }
        },
      }),
      sessionClient,
    ],
    polyfill: false,
  })

  return { client, sessionClient }
}

// ── Request command ─────────────────────────────────────────────────

async function handleRequest(args: string[]): Promise<void> {
  const cfg = requireConfig()

  // Parse curl-like args
  let method = 'GET'
  let body: string | undefined
  let url = ''
  let dryRun = false
  const headers: Record<string, string> = {}

  for (let i = 0; i < args.length; i++) {
    if (args[i] === '-X' && args[i + 1]) method = args[++i]
    else if (args[i] === '--json' && args[i + 1]) { body = args[++i]; method = method === 'GET' ? 'POST' : method }
    else if (args[i] === '-H' && args[i + 1]) { const [k, ...v] = args[++i].split(':'); headers[k.trim()] = v.join(':').trim() }
    else if (args[i] === '--dry-run') dryRun = true
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

  console.error(`> ${method} ${url}`)

  // Restore active session if one exists for this server
  const baseUrl = new URL(url).origin
  const { client: mppxClient, sessionClient } = createMppxClient(cfg)

  const persistedSession = loadSession()
  if (persistedSession && persistedSession.url === baseUrl) {
    // Hydrate the session client with persisted state
    // The session client will use bearer credentials automatically
    sessionClient.restore(persistedSession.sessionId, persistedSession.bearer)
    console.error(`  Active session: ${persistedSession.sessionId}`)
  }

  // mppx.fetch handles the full flow:
  // - If free endpoint: returns response directly
  // - If 402 charge: calls zcashClient.createPayment → wallet send → retry
  // - If 402 session: calls zcashSessionClient.sendPayment → deposit → open
  // - If active session: sends bearer credential automatically
  const resp = await mppxClient.fetch(url, { method, headers, body })

  const result = await resp.text()

  // Extract session ID from response body if a session was opened
  const activeSession = sessionClient.getSession()
  if (activeSession && !activeSession.sessionId) {
    // Session was just opened — extract sessionId from server response
    try {
      const body = JSON.parse(result)
      if (body.sessionId) {
        sessionClient.setSessionId(body.sessionId)
        console.error(`  Session opened: ${body.sessionId}`)
      }
    } catch { /* not JSON or no sessionId */ }
  }

  // Persist session state
  const session = sessionClient.getSession()
  if (session && session.sessionId) {
    saveSession({
      sessionId: session.sessionId,
      bearer: session.bearer,
      url: baseUrl,
      endpoint: new URL(url).pathname,
    })
  }

  if (resp.status === 200) {
    console.error('')
    console.error(`  --- Payment Complete ---`)
    console.error(`  Status:  Verified`)
    console.error(`  Privacy: Fully shielded (Orchard)`)
    console.error(`  ---`)
    console.error('')
  } else {
    console.error(`  ${resp.status}`)
  }

  console.log(result)
}

// ── Session commands ────────────────────────────────────────────────

async function handleSessionClose(): Promise<void> {
  const session = loadSession()
  if (!session) {
    console.error('No active session.')
    return
  }

  const cfg = requireConfig()
  console.error(`Closing session ${session.sessionId}...`)

  const { client: mppxClient, sessionClient } = createMppxClient(cfg)
  sessionClient.restore(session.sessionId, session.bearer)
  sessionClient.close()

  const closePath = session.endpoint ?? '/api/session/fortune'
  const closeUrl = `${session.url}${closePath}`

  const resp = await mppxClient.fetch(closeUrl)

  clearSession()

  if (resp.status === 200 || resp.status === 204) {
    const receiptHeader = resp.headers.get('payment-receipt') ?? resp.headers.get('Payment-Receipt') ?? ''
    console.log(`--- Session Closed ---`)
    console.log(`  Session:  ${session.sessionId}`)
    console.log(`  Status:   Closed`)
    if (receiptHeader) console.log(`  Receipt:  ${receiptHeader.slice(0, 40)}...`)
    console.log(`---`)
  } else {
    const body = await resp.text()
    console.error(`Close failed: ${resp.status} ${body}`)
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
  zimppy wallet create [name]    Create a new wallet (default: "default")
  zimppy wallet restore <seed>   Restore wallet from seed phrase
  zimppy wallet list             List all wallets
  zimppy wallet use <name>       Switch active wallet
  zimppy wallet whoami           Show address + balance + network
  zimppy wallet balance          Show balance
  zimppy wallet seed             Show seed phrase (for backup)
  zimppy wallet fund             How to add funds
  zimppy wallet services         List available paid services
  zimppy request <URL>           Make HTTP request with auto-pay
  zimppy request --dry-run <URL> Show what would be sent
  zimppy --version               Show version

Request options:
  -X METHOD        HTTP method (default: GET)
  --json '{...}'   Send JSON body (implies POST)
  -H 'Key: Val'    Add header
  --dry-run        Show request without sending
  -m <seconds>     Timeout

Examples:
  zimppy wallet whoami
  zimppy wallet services --search fortune
  zimppy request http://localhost:3180/api/fortune
  zimppy request -X GET http://localhost:3180/api/fortune`)
    return
  }

  if (cmd === '--version' || cmd === '-v') {
    console.log(VERSION)
    return
  }

  if (cmd === 'wallet') {
    const sub = args[1]
    switch (sub) {
      case 'create': return walletCreate(args.slice(2))
      case 'restore': return walletRestore(args.slice(2))
      case 'list': return walletList()
      case 'use': return walletUse(args[2])
      case 'whoami': return walletWhoami()
      case 'balance': return walletBalance()
      case 'seed': return walletSeed()
      case 'fund': return walletFund()
      case 'services': return walletServices(args.slice(2))
      default:
        console.error(`Unknown wallet command: ${sub}`)
        console.error('Available: create, restore, list, use, whoami, balance, seed, fund, services')
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
