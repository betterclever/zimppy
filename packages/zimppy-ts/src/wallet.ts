/**
 * Wallet resolution — reads wallet config from ~/.zimppy/
 *
 * Users specify a wallet name, and this module resolves it to
 * the full config needed by server/client methods.
 */

import { readFileSync, existsSync } from 'node:fs'
import { join } from 'node:path'
import { homedir } from 'node:os'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)

export interface ZimppyConfig {
  dataDir: string
  lwdServer: string
  rpcEndpoint: string
  network: 'testnet' | 'mainnet'
  activeWallet?: string
  address?: string
}

export interface ResolvedWallet {
  dataDir: string
  lwdServer: string
  rpcEndpoint: string
  network: 'testnet' | 'mainnet'
  address: string
  orchardIvk: string
}

const ZIMPPY_DIR = join(homedir(), '.zimppy')
const CONFIG_PATH = join(ZIMPPY_DIR, 'config.json')

/**
 * Read the global zimppy config file.
 */
function readConfig(): ZimppyConfig {
  if (!existsSync(CONFIG_PATH)) {
    throw new Error(
      `No zimppy config found at ${CONFIG_PATH}. Run "npx zimppy wallet create" first.`,
    )
  }
  return JSON.parse(readFileSync(CONFIG_PATH, 'utf-8')) as ZimppyConfig
}

/**
 * Resolve a wallet by name. Opens the wallet via NAPI to get address and IVK.
 *
 * @param name - Wallet name (e.g., "default", "server"). If omitted, uses the active wallet.
 */
export async function resolveWallet(name?: string): Promise<ResolvedWallet> {
  const config = readConfig()
  const walletName = name ?? config.activeWallet ?? 'default'
  const dataDir = join(ZIMPPY_DIR, 'wallets', walletName)

  if (!existsSync(dataDir)) {
    throw new Error(
      `Wallet "${walletName}" not found at ${dataDir}. Run "npx zimppy wallet create ${walletName}" first.`,
    )
  }

  const { ZimppyWalletNapi } = require('zimppy-napi') as { ZimppyWalletNapi: any }

  const wallet = await ZimppyWalletNapi.open(
    dataDir,
    config.lwdServer,
    config.network,
  )

  const [address, orchardIvk, walletNetwork] = await Promise.all([
    wallet.address(),
    wallet.orchardIvk(),
    Promise.resolve(wallet.network() as string),
  ])

  const network = (walletNetwork === 'mainnet' ? 'mainnet' : 'testnet') as 'testnet' | 'mainnet'

  return {
    dataDir,
    lwdServer: config.lwdServer,
    rpcEndpoint: config.rpcEndpoint,
    network,
    address,
    orchardIvk,
  }
}

/**
 * Open a wallet by name for sending payments. Returns the NAPI wallet instance.
 */
export async function openWallet(name?: string) {
  const config = readConfig()
  const walletName = name ?? config.activeWallet ?? 'default'
  const dataDir = join(ZIMPPY_DIR, 'wallets', walletName)

  if (!existsSync(dataDir)) {
    throw new Error(
      `Wallet "${walletName}" not found at ${dataDir}. Run "npx zimppy wallet create ${walletName}" first.`,
    )
  }

  const { ZimppyWalletNapi } = require('zimppy-napi') as { ZimppyWalletNapi: any }

  const wallet = await ZimppyWalletNapi.open(
    dataDir,
    config.lwdServer,
    config.network,
  )

  return { wallet, config, walletName }
}
