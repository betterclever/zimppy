/**
 * SSE Streamed Payments — pay-per-token metered streaming.
 *
 * The server streams data (e.g., LLM tokens) and deducts from the
 * session balance per chunk. When balance runs out, it pauses the
 * stream and emits a `payment-need-voucher` event. The client sends
 * a topUp, and the stream resumes.
 *
 * Event types:
 *   event: message        → data chunk (the actual content)
 *   event: payment-need-topup → balance exhausted, client must topUp
 *   event: payment-receipt → stream complete, final receipt
 */

export interface ServeStreamOptions {
  /** Async generator that yields string chunks (e.g., LLM tokens) */
  generate: AsyncIterable<string>
  /** Session ID for balance tracking */
  sessionId: string
  /** Cost per chunk in zatoshis */
  tickCost: number
  /** Function to deduct balance — returns remaining balance, throws if insufficient */
  deduct: (sessionId: string, amount: number) => Promise<number>
  /** Function to get current balance */
  getBalance: (sessionId: string) => Promise<number>
  /** Max milliseconds to wait for a topUp before aborting */
  topUpTimeoutMs?: number
  /** Poll interval for checking topUp arrival */
  pollIntervalMs?: number
}

export interface NeedTopupEvent {
  sessionId: string
  balanceRequired: number
  balanceSpent: number
}

/** @deprecated Use NeedTopupEvent */
export type NeedVoucherEvent = NeedTopupEvent

export interface StreamReceipt {
  sessionId: string
  totalSpent: number
  totalChunks: number
}

/**
 * Create an SSE ReadableStream that meters content delivery against session balance.
 */
export function serveStream(options: ServeStreamOptions): ReadableStream<string> {
  const {
    generate,
    sessionId,
    tickCost,
    deduct,
    getBalance,
    topUpTimeoutMs = 300_000,
    pollIntervalMs = 1_000,
  } = options

  let totalSpent = 0
  let totalChunks = 0

  return new ReadableStream<string>({
    async start(controller) {
      try {
        for await (const chunk of generate) {
          // Try to deduct
          let remaining: number
          try {
            remaining = await deduct(sessionId, tickCost)
            totalSpent += tickCost
            totalChunks++
          } catch {
            // Balance exhausted — emit need-topup and wait
            const balance = await getBalance(sessionId).catch(() => 0)
            const needTopup: NeedTopupEvent = {
              sessionId,
              balanceRequired: tickCost,
              balanceSpent: balance,
            }
            controller.enqueue(formatSSE('payment-need-topup', JSON.stringify(needTopup)))

            // Poll for topUp
            const deadline = Date.now() + topUpTimeoutMs
            let funded = false
            while (Date.now() < deadline) {
              await sleep(pollIntervalMs)
              try {
                remaining = await deduct(sessionId, tickCost)
                totalSpent += tickCost
                totalChunks++
                funded = true
                break
              } catch {
                // Still no balance, keep waiting
              }
            }

            if (!funded) {
              controller.enqueue(formatSSE('error', JSON.stringify({ error: 'topUp timeout' })))
              break
            }
          }

          // Emit the content chunk
          controller.enqueue(formatSSE('message', chunk))
        }

        // Stream complete — emit receipt
        const receipt: StreamReceipt = { sessionId, totalSpent, totalChunks }
        controller.enqueue(formatSSE('payment-receipt', JSON.stringify(receipt)))
        controller.close()
      } catch (err) {
        controller.enqueue(formatSSE('error', JSON.stringify({ error: (err as Error).message })))
        controller.close()
      }
    },
  })
}

function formatSSE(event: string, data: string): string {
  return `event: ${event}\ndata: ${data}\n\n`
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}
