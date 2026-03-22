/**
 * SSE Streamed Payments — pay-per-token metered streaming.
 *
 * The server streams data (e.g., LLM tokens) and deducts from the
 * session balance per chunk. When balance runs out, it pauses the
 * stream and emits a `payment-need-topup` event. The client sends
 * a topUp, and the stream resumes.
 *
 * Event types:
 *   event: message             → data chunk (the actual content)
 *   event: payment-need-topup  → balance exhausted, client must topUp
 *   event: payment-receipt     → stream complete, final receipt
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
  /** AbortSignal to cancel the stream */
  signal?: AbortSignal
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
 * Returns a ReadableStream<Uint8Array> suitable for use as an HTTP response body.
 */
export function serveStream(options: ServeStreamOptions): ReadableStream<Uint8Array> {
  const {
    generate,
    sessionId,
    tickCost,
    deduct,
    getBalance,
    topUpTimeoutMs = 300_000,
    pollIntervalMs = 1_000,
    signal,
  } = options

  const encoder = new TextEncoder()
  let totalSpent = 0
  let totalChunks = 0

  return new ReadableStream<Uint8Array>({
    async start(controller) {
      const aborted = () => signal?.aborted ?? false
      const emit = (event: string) => controller.enqueue(encoder.encode(event))

      try {
        for await (const chunk of generate) {
          if (aborted()) break

          // Try to deduct
          try {
            await deduct(sessionId, tickCost)
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
            emit(formatSSE('payment-need-topup', JSON.stringify(needTopup)))

            // Poll for topUp
            const deadline = Date.now() + topUpTimeoutMs
            let funded = false
            while (Date.now() < deadline) {
              if (aborted()) break
              await sleep(pollIntervalMs)
              try {
                await deduct(sessionId, tickCost)
                totalSpent += tickCost
                totalChunks++
                funded = true
                break
              } catch {
                // Still no balance, keep waiting
              }
            }

            if (!funded) {
              emit(formatSSE('error', JSON.stringify({ error: 'topUp timeout' })))
              break
            }
          }

          // Emit the content chunk
          emit(formatSSE('message', chunk))
        }

        // Stream complete — emit receipt
        if (!aborted()) {
          const receipt: StreamReceipt = { sessionId, totalSpent, totalChunks }
          emit(formatSSE('payment-receipt', JSON.stringify(receipt)))
        }
      } catch (err) {
        if (!aborted()) {
          emit(formatSSE('error', JSON.stringify({ error: (err as Error).message })))
        }
      } finally {
        controller.close()
      }
    },
  })
}

/**
 * Wrap a ReadableStream<Uint8Array> (from serveStream) in an HTTP
 * Response with the correct SSE headers.
 */
export function toResponse(body: ReadableStream<Uint8Array>): Response {
  return new Response(body, {
    headers: {
      'Cache-Control': 'no-cache, no-transform',
      Connection: 'keep-alive',
      'Content-Type': 'text/event-stream; charset=utf-8',
    },
  })
}

/**
 * Parsed SSE event (discriminated union by type).
 */
export type SseEvent =
  | { type: 'message'; data: string }
  | { type: 'payment-need-topup'; data: NeedTopupEvent }
  | { type: 'payment-receipt'; data: StreamReceipt }

/**
 * Parse a raw SSE event string into a typed event.
 *
 * Handles the three event types used by zimppy streaming:
 * - message (default / no event field) — application data
 * - payment-need-topup — balance exhausted, client should topUp
 * - payment-receipt — final receipt
 */
export function parseEvent(raw: string): SseEvent | null {
  let eventType = 'message'
  const dataLines: string[] = []

  for (const line of raw.split('\n')) {
    if (line.startsWith('event: ')) {
      eventType = line.slice(7).trim()
    } else if (line.startsWith('data: ')) {
      dataLines.push(line.slice(6))
    } else if (line === 'data:') {
      dataLines.push('')
    }
  }

  if (dataLines.length === 0) return null
  const data = dataLines.join('\n')

  switch (eventType) {
    case 'message':
      return { type: 'message', data }
    case 'payment-need-topup':
      return { type: 'payment-need-topup', data: JSON.parse(data) as NeedTopupEvent }
    case 'payment-receipt':
      return { type: 'payment-receipt', data: JSON.parse(data) as StreamReceipt }
    default:
      return { type: 'message', data }
  }
}

/**
 * Check whether a Response carries an SSE event stream.
 */
export function isEventStream(response: Response): boolean {
  const ct = response.headers.get('content-type')
  return ct?.toLowerCase().startsWith('text/event-stream') ?? false
}

/**
 * Parse an SSE Response body into an async iterable of data payloads.
 *
 * Yields the raw data field content for each SSE message event.
 * Events whose data matches the skip predicate are silently dropped
 * (e.g. [DONE] sentinels used by OpenAI-compatible APIs).
 */
export async function* iterateData(
  response: Response,
  options?: { skip?: (data: string) => boolean },
): AsyncGenerator<string> {
  const skip = options?.skip
  const body = response.body
  if (!body) return

  const reader = body.getReader()
  const decoder = new TextDecoder()
  let buffer = ''

  try {
    while (true) {
      const { value, done } = await reader.read()
      if (done) break

      buffer += decoder.decode(value, { stream: true })

      // Split on double-newline SSE event boundaries
      const events = buffer.split('\n\n')
      // Last element may be incomplete — keep in buffer
      buffer = events.pop() ?? ''

      for (const event of events) {
        if (!event.trim()) continue
        const parsed = parseEvent(event)
        if (!parsed || parsed.type !== 'message') continue
        if (skip?.(parsed.data)) continue
        yield parsed.data
      }
    }

    // Flush remaining buffer
    if (buffer.trim()) {
      const parsed = parseEvent(buffer)
      if (parsed?.type === 'message' && !skip?.(parsed.data)) {
        yield parsed.data
      }
    }
  } finally {
    reader.releaseLock()
  }
}

function formatSSE(event: string, data: string): string {
  return `event: ${event}\ndata: ${data}\n\n`
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}
