import { connect, Socket } from 'node:net'
import type { Transport } from '@modelcontextprotocol/sdk/shared/transport.js'
import type { JSONRPCMessage } from '@modelcontextprotocol/sdk/types.js'

export class SocketTransport implements Transport {
  private buffer = ''
  constructor(private readonly socket: Socket) {}
  onclose?: () => void
  onerror?: (error: Error) => void
  onmessage?: (message: JSONRPCMessage) => void
  sessionId?: string

  static connect(port: number, host = '127.0.0.1') {
    const socket = connect(port, host)
    return new SocketTransport(socket)
  }

  async start(): Promise<void> {
    this.socket.setEncoding('utf8')
    this.socket.on('data', (chunk: string | Buffer) => {
      this.buffer += chunk.toString()
      this.flushBuffer()
    })
    this.socket.on('error', (error) => {
      this.onerror?.(error)
    })
    this.socket.on('close', () => {
      this.onclose?.()
    })

    if (!this.socket.connecting) {
      return
    }

    await new Promise<void>((resolve, reject) => {
      this.socket.once('connect', () => resolve())
      this.socket.once('error', reject)
    })
  }

  async send(message: JSONRPCMessage): Promise<void> {
    await new Promise<void>((resolve, reject) => {
      this.socket.write(`${JSON.stringify(message)}\n`, (error) => {
        if (error) {
          reject(error)
          return
        }
        resolve()
      })
    })
  }

  async close(): Promise<void> {
    this.socket.end()
    this.buffer = ''
  }

  private flushBuffer(): void {
    while (true) {
      const newlineIndex = this.buffer.indexOf('\n')
      if (newlineIndex === -1) {
        return
      }

      const line = this.buffer.slice(0, newlineIndex).trim()
      this.buffer = this.buffer.slice(newlineIndex + 1)
      if (!line) {
        continue
      }

      try {
        this.onmessage?.(JSON.parse(line) as JSONRPCMessage)
      } catch (error) {
        this.onerror?.(error as Error)
      }
    }
  }
}
