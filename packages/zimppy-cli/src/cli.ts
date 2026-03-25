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
const { zcashClient, zcashSessionClient, isEventStream, parseEvent } = require('zimppy-ts') as typeof import('zimppy-ts')

const VERSION = '0.1.0'
const CONFIG_DIR = process.env.ZIMPPY_HOME ?? join(homedir(), '.zimppy')
const WALLETS_DIR = join(CONFIG_DIR, 'wallets')
const CONFIG_FILE = join(CONFIG_DIR, 'config.json')
const SESSION_FILE = join(CONFIG_DIR, 'session.json')
const DEFAULT_LWD = 'https://testnet.zec.rocks'
const DEFAULT_RPC = 'https://zcash-testnet-zebrad.gateway.tatum.io'
const DEFAULT_DEPOSIT = '100000' // 100,000 zat default session deposit
const SPINNER_FRAMES = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏']

// ── Wallet NAPI ──────────────────────────────────────────────────

async function openWallet(cfg: ZimppyConfig, seedPhrase?: string): Promise<ZimppyWalletNapi> {
  const { ZimppyWalletNapi: Wallet } = require('@zimppy/core-napi') as typeof import('@zimppy/core-napi')
  if (seedPhrase) {
    throw new Error('openWallet only opens existing wallets; use Wallet.restore for seed restores')
  }
  return Wallet.open(cfg.dataDir, cfg.lwdServer, cfg.network)
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

function startProgress(label: string) {
  const start = Date.now()
  let frameIndex = 0
  let message = label
  const render = () => {
    const elapsed = Math.round((Date.now() - start) / 1000)
    process.stderr.write(`\r\x1b[K  \x1b[36m${SPINNER_FRAMES[frameIndex]}\x1b[0m ${message} \x1b[2m${elapsed}s\x1b[0m`)
    frameIndex = (frameIndex + 1) % SPINNER_FRAMES.length
  }

  render()
  const timer = setInterval(render, 80)

  return {
    update(msg: string) {
      message = msg
    },
    stop(msg?: string) {
      clearInterval(timer)
      const elapsed = Math.round((Date.now() - start) / 1000)
      process.stderr.write(`\r\x1b[K  \x1b[32;1m✓\x1b[0m ${msg ?? label} \x1b[2m${elapsed}s\x1b[0m\n`)
    },
    fail(msg: string) {
      clearInterval(timer)
      const elapsed = Math.round((Date.now() - start) / 1000)
      process.stderr.write(`\r\x1b[K  \x1b[31;1m✗\x1b[0m ${label} \x1b[2m${elapsed}s\x1b[0m\n`)
      process.stderr.write(`    ${msg}\n`)
    },
  }
}

async function syncWalletWithProgress(
  wallet: ZimppyWalletNapi,
  label: string,
): Promise<void> {
  const progress = startProgress(label)
  try {
    await wallet.ensureReady()
    progress.stop(label)
  } catch (error) {
    progress.fail((error as Error).message)
    throw error
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
    const { ZimppyWalletNapi: Wallet } = require('@zimppy/core-napi') as typeof import('@zimppy/core-napi')
    const wallet = await Wallet.create(config.dataDir, config.lwdServer, config.network, null)
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
  const birthday = args[2] ? Number(args[2]) : undefined
  if (birthday === undefined || Number.isNaN(birthday)) {
    console.error('Usage: zimppy wallet restore "your 24 word seed phrase" [name] <birthday>')
    console.error('Restore requires the wallet birthday height.')
    process.exit(1)
  }
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
    const { ZimppyWalletNapi: Wallet } = require('@zimppy/core-napi') as typeof import('@zimppy/core-napi')
    const wallet = await Wallet.restore(config.dataDir, config.lwdServer, config.network, phrase, birthday)
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

  let wallet: ZimppyWalletNapi | null = null
  try {
    wallet = await openWallet(cfg)
    await syncWalletWithProgress(wallet, 'Syncing wallet')

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
  } finally {
    await wallet?.close().catch(() => {})
  }
}

async function walletBalance(): Promise<void> {
  const cfg = requireConfig()
  let wallet: ZimppyWalletNapi | null = null
  try {
    wallet = await openWallet(cfg)
    await syncWalletWithProgress(wallet, 'Syncing wallet')

    const bal = await wallet.balance()

    console.log(`--- Wallet Balance ---`)
    console.log(`  Spendable: ${bal.spendableZat} zat`)
    console.log(`  Pending:   ${bal.pendingZat} zat`)
    console.log(`  Total:     ${bal.totalZat} zat`)
    console.log(`  Network:   ${cfg.network}`)
    console.log(`---`)
  } catch (e) {
    console.error(`ERROR: ${(e as Error).message}`)
  } finally {
    await wallet?.close().catch(() => {})
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
  let wallet: ZimppyWalletNapi | null = null
  try {
    wallet = await openWallet(cfg)
    const seed = await wallet.seedPhrase()
    if (seed) {
      console.log(seed)
    } else {
      console.error('Seed phrase not available (watch-only wallet)')
    }
  } catch (e) {
    console.error(`ERROR: ${(e as Error).message}`)
  } finally {
    await wallet?.close().catch(() => {})
  }
}

const ZEC_FEE = 10_000
const BLOCK_TIME_S = 90 // ~75s target, 90s conservative
const POLL_INTERVAL_S = 15

/**
 * Wait until wallet has enough spendable balance.
 * - Checks `spendable >= needed` (not just pending == 0)
 * - Upper bound based on min_confirmations * block time * 2
 * - Early exit if no pending tx and balance is genuinely too low
 */
async function waitForSpendable(
  wallet: ZimppyWalletNapi,
  label: string,
  needed: number,
  minConf: number = 3,
): Promise<void> {
  const maxPolls = Math.max(20, Math.ceil((minConf * BLOCK_TIME_S * 2) / POLL_INTERVAL_S))
  const earlyExitThreshold = Math.ceil((minConf * BLOCK_TIME_S * 2) / POLL_INTERVAL_S)
  const estSecs = minConf * 75
  let noProgressCount = 0
  const started = Date.now()
  const progress = startProgress(label)

  for (let attempt = 1; attempt <= maxPolls; attempt++) {
    await new Promise(r => setTimeout(r, POLL_INTERVAL_S * 1000))
    await wallet.sync()
    const bal = await wallet.balance()
    const spendable = Number(bal.spendableZat)
    const pending = Number(bal.pendingZat)
    const elapsed = Math.round((Date.now() - started) / 1000)
    const remaining = Math.max(0, estSecs - elapsed)

    if (pending > 0) {
      progress.update(`Confirming... ~${remaining}s remaining`)
    } else {
      progress.update(`Waiting for maturity... ${elapsed}s elapsed`)
    }

    if (spendable >= needed) {
      progress.stop('Confirmed')
      return
    }

    if (pending === 0 && spendable < needed) {
      noProgressCount++
      if (noProgressCount > earlyExitThreshold) {
        progress.fail('no pending transactions and balance too low')
        throw new Error(`Insufficient balance: have ${spendable}, need ${needed}. No pending transactions to wait for.`)
      }
    } else {
      noProgressCount = 0
    }
  }
  progress.fail('timed out waiting for confirmation')
  throw new Error(`Timed out waiting for spendable balance >= ${needed}`)
}

async function walletSend(args: string[]): Promise<void> {
  // Parse flags
  const noWait = args.includes('--no-wait')
  let minConf: number | undefined
  const minConfIdx = args.indexOf('--min-conf')
  if (minConfIdx !== -1 && args[minConfIdx + 1]) {
    minConf = Number(args[minConfIdx + 1])
  }
  const filtered = args.filter((a, i) =>
    a !== '--no-wait' && a !== '--min-conf' && (minConfIdx === -1 || i !== minConfIdx + 1)
  )

  const to = filtered[0]
  const amountZat = filtered[1]
  const memo = filtered[2] ?? undefined

  if (!to || !amountZat) {
    console.error('Usage: zimppy wallet send <address> <amount_zat> [memo] [--no-wait] [--min-conf N]')
    console.error('')
    console.error('  --no-wait      Exit after broadcast without waiting for confirmation')
    console.error('  --min-conf N   Override min confirmations (default: wallet setting, usually 3)')
    process.exit(1)
  }

  const cfg = requireConfig()
  const needed = Number(amountZat) + ZEC_FEE

  let wallet: ZimppyWalletNapi | null = null
  try {
    wallet = await openWallet(cfg)
    if (minConf !== undefined) {
      await wallet.setMinConfirmations(minConf)
    }
    await syncWalletWithProgress(wallet, 'Syncing')

    const bal = await wallet.balance()
    const spendZec = (Number(bal.spendableZat) / 100_000_000).toFixed(4)
    console.error(`  Spendable: \x1b[36m${bal.spendableZat} zat (${spendZec} ZEC)\x1b[0m`)

    const effectiveMinConf = minConf ?? 3
    const maxRetries = Math.max(24, Math.ceil((effectiveMinConf * BLOCK_TIME_S * 2) / POLL_INTERVAL_S))

    const shortAddr = to.length > 40 ? `${to.slice(0, 20)}...${to.slice(-12)}` : to
    let txid: string | null = null
    const sendStarted = Date.now()
    const sp = startProgress(`Sending ${amountZat} zat to ${shortAddr}`)

    for (let attempt = 0; attempt <= maxRetries; attempt++) {
      if (attempt > 0) {
        // Wait POLL_INTERVAL but let the spinner tick elapsed per-second
        await new Promise(r => setTimeout(r, POLL_INTERVAL_S * 1000))
        const elapsed = Math.round((Date.now() - sendStarted) / 1000)
        sp.update(`Waiting for prior change to mature... ${elapsed}s elapsed`)
        await wallet.sync()
        const b = await wallet.balance()
        if (Number(b.totalZat) < needed) {
          sp.fail('Insufficient total balance')
          process.exit(1)
        }
      }
      try {
        txid = await wallet.send(to, amountZat, memo ?? null)
        break
      } catch (error) {
        const msg = (error as Error).message
        if (msg.includes('Insufficient balance') && Number((await wallet.balance()).totalZat) >= needed) {
          continue
        }
        sp.fail((error as Error).message)
        throw error
      }
    }
    sp.stop()

    if (!txid) {
      console.error('\x1b[31;1m✗\x1b[0m Timed out waiting for change to mature')
      process.exit(1)
    }
    console.error(`  \x1b[32;1m✓\x1b[0m Broadcast: \x1b[2m${txid}\x1b[0m`)

    // Post-send: wait for this tx's change to mature (unless --no-wait)
    if (!noWait) {
      await waitForSpendable(wallet, 'Awaiting confirmation', needed, effectiveMinConf)
      const postBal = await wallet.balance()
      const zec = (Number(postBal.spendableZat) / 100_000_000).toFixed(4)
      console.error(`  Spendable: \x1b[36m${postBal.spendableZat} zat (${zec} ZEC)\x1b[0m`)
    }

    console.error('  \x1b[32;1m✓\x1b[0m Send complete.')
  } catch (e) {
    console.error(`ERROR: ${(e as Error).message}`)
    process.exit(1)
  } finally {
    await wallet?.close().catch(() => {})
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
  const sp = startProgress('Waiting for block confirmation')
  const estSecs = 75

  for (let i = 0; i < 20; i++) {
    await new Promise(r => setTimeout(r, 15000))
    const remaining = Math.max(0, estSecs - (i + 1) * 15)
    sp.update(`Confirming on-chain... ~${remaining}s remaining`)
    try {
      const resp = await fetch(cfg.rpcEndpoint, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ jsonrpc: '2.0', method: 'getrawtransaction', params: [txid, 1], id: 1 }),
      })
      const data = await resp.json() as { result?: { confirmations?: number } }
      if (data.result?.confirmations && data.result.confirmations > 0) {
        sp.stop(`Confirmed (${data.result.confirmations} confirmations)`)
        return
      }
    } catch { /* keep polling */ }
  }
  sp.stop('Confirmation timeout — tx may still confirm later')
}

async function sendViaWallet(cfg: ZimppyConfig, params: { to: string; amountZat: string; memo: string }): Promise<string> {
  let wallet: ZimppyWalletNapi | null = null
  try {
    wallet = await openWallet(cfg)
    await syncWalletWithProgress(wallet, 'Syncing wallet')

    const sp = startProgress(`Sending ${params.amountZat} zat`)
    let txid: string
    try {
      txid = await wallet.send(params.to, params.amountZat, params.memo)
      sp.stop('Broadcast')
    } catch (error) {
      sp.fail((error as Error).message)
      throw error
    }
    console.error(`  \x1b[32;1m✓\x1b[0m txid: \x1b[2m${txid.slice(0, 16)}...${txid.slice(-8)}\x1b[0m`)

    await waitForConfirmation(cfg, txid)
    await syncWalletWithProgress(wallet, 'Post-confirmation sync')

    return txid
  } finally {
    await wallet?.close().catch(() => {})
  }
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
        createPayment: async ({ challenge, challengeId }) => {
          const memo = (challenge.methodDetails?.memo ?? '').replace('{id}', challengeId)
          console.error('')
          console.error(`  --- Charge ---`)
          console.error(`  Amount:    ${challenge.amount} zat`)
          console.error(`  Recipient: ${challenge.recipient.slice(0, 40)}...`)
          console.error(`  Memo:      ${memo.slice(0, 40)}...`)
          console.error(`  ---`)
          console.error('')

          const txid = await sendViaWallet(cfg, {
            to: challenge.recipient,
            amountZat: challenge.amount,
            memo,
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

async function renderEventStream(
  resp: Response,
  url: string,
  method: string,
  headers: Record<string, string>,
  body: string | undefined,
  mppxClient: Awaited<ReturnType<typeof Mppx.create>>,
  sessionClient: ReturnType<typeof zcashSessionClient>,
): Promise<void> {
  const reader = resp.body?.getReader()
  if (!reader) return

  const decoder = new TextDecoder()
  let buffer = ''

  while (true) {
    const { value, done } = await reader.read()
    if (done) break

    buffer += decoder.decode(value, { stream: true })
    const events = buffer.split('\n\n')
    buffer = events.pop() ?? ''

    for (const raw of events) {
      if (!raw.trim()) continue
      const parsed = parseEvent(raw)
      if (!parsed) continue

      if (parsed.type === 'message') {
        try {
          const data = JSON.parse(parsed.data) as { token?: string }
          process.stdout.write(data.token ? `${data.token} ` : parsed.data)
        } catch {
          process.stdout.write(parsed.data)
        }
        continue
      }

      if (parsed.type === 'payment-need-topup') {
        console.error('')
        console.error(`  Balance exhausted, topping up ${parsed.data.balanceRequired} zat...`)
        sessionClient.topUp()
        const topUpResp = await mppxClient.fetch(url, { method, headers, body })
        if (!(topUpResp.status === 200 || topUpResp.status === 204)) {
          const topUpBody = await topUpResp.text()
          throw new Error(`top-up failed: ${topUpResp.status} ${topUpBody}`)
        }
        console.error('  Top-up accepted, resuming stream')
        continue
      }

      if (parsed.type === 'payment-receipt') {
        console.error('')
        console.error(`  Stream receipt: ${parsed.data.totalSpent} zat over ${parsed.data.totalChunks} chunks`)
      }
    }
  }

  if (buffer.trim()) {
    const parsed = parseEvent(buffer)
    if (parsed?.type === 'message') {
      process.stdout.write(parsed.data)
    }
  }
  process.stdout.write('\n')
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

  if (isEventStream(resp)) {
    const session = sessionClient.getSession()
    if (session && session.sessionId) {
      saveSession({
        sessionId: session.sessionId,
        bearer: session.bearer,
        url: baseUrl,
        endpoint: new URL(url).pathname,
      })
    }
    await renderEventStream(resp, url, method, headers, body, mppxClient, sessionClient)
    return
  }

  const result = await resp.text()

  const activeSession = sessionClient.getSession()
  if (activeSession && !activeSession.sessionId) {
    try {
      const responseBody = JSON.parse(result)
      if (responseBody.sessionId) {
        sessionClient.setSessionId(responseBody.sessionId)
        console.error(`  Session opened: ${responseBody.sessionId}`)
      }
    } catch { /* not JSON or no sessionId */ }
  }

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
  zimppy wallet send <addr> <zat> Send ZEC (--wait to block until confirmed)
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
      case 'send': return walletSend(args.slice(2))
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
