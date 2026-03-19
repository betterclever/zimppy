import { readFileSync } from 'node:fs'
import { fetchTxStatus, RPC_ENDPOINT, TX_TRACK_FILE, sleep } from './autopay.js'

type RpcEnvelope<T> = {
  error?: { message?: string }
  result?: T
}

const WATCHER_POLL_MS = Number(process.env.ZIMPPY_WATCHER_POLL_MS ?? 15_000)

async function main(): Promise<void> {
  console.log('=== Chain Progress Watcher ===')
  console.log(`RPC: ${RPC_ENDPOINT}`)
  console.log(`Tracking file: ${TX_TRACK_FILE}`)
  console.log(`Poll interval: ${Math.round(WATCHER_POLL_MS / 1000)}s`)
  console.log()

  let lastHeight = -1
  let lastTxid = ''
  let txFirstSeenAt = 0

  while (true) {
    try {
      const height = await fetchBlockHeight()
      const tipDelta = lastHeight === -1 ? 'init' : `${height - lastHeight}`
      lastHeight = height

      const txid = readTrackedTx()
      if (txid && txid !== lastTxid) {
        lastTxid = txid
        txFirstSeenAt = Date.now()
        console.log(`[tx] tracking ${txid}`)
      }

      if (txid) {
        const status = await fetchTxStatus(txid)
        const ageSeconds = txFirstSeenAt ? Math.floor((Date.now() - txFirstSeenAt) / 1000) : 0
        console.log(
          `[watch] tip=${height} (delta=${tipDelta}) tx=${shortTxid(txid)} state=${status.state} conf=${status.confirmations ?? 'n/a'} age=${ageSeconds}s`,
        )
      } else {
        console.log(`[watch] tip=${height} (delta=${tipDelta}) tx=none`)
      }
    } catch (error) {
      const message = (error as Error).message
      console.log(`[watcher] ${message}`)
    }

    await sleep(WATCHER_POLL_MS)
  }
}

async function fetchBlockHeight(): Promise<number> {
  const payload = await rpcCall<{ blocks?: number }>('getblockchaininfo', [])
  if (!payload) {
    throw new Error('getblockchaininfo returned no result')
  }
  return payload.blocks ?? -1
}

async function rpcCall<T>(
  method: string,
  params: unknown[],
  allowMissing = false,
): Promise<T | null> {
  const response = await fetch(RPC_ENDPOINT, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      jsonrpc: '2.0',
      id: 1,
      method,
      params,
    }),
  })

  const payload = await response.json() as RpcEnvelope<T>
  if (payload.error) {
    const message = payload.error.message ?? 'unknown rpc error'
    if (allowMissing && /no such mempool or main chain transaction/i.test(message)) {
      return null
    }
    throw new Error(`${method}: ${message}`)
  }
  return payload.result ?? null
}

function readTrackedTx(): string {
  try {
    return readFileSync(TX_TRACK_FILE, 'utf8').trim()
  } catch {
    return ''
  }
}

function shortTxid(txid: string): string {
  return txid.length > 16 ? `${txid.slice(0, 8)}...${txid.slice(-8)}` : txid
}

main().catch((error: unknown) => {
  console.error(error instanceof Error ? error.stack ?? error.message : String(error))
  process.exitCode = 1
})
