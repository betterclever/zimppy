import { createServer } from 'node:net'
import { readFileSync } from 'node:fs'
import { createMcpApp } from './app.js'
import { SocketTransport } from './socket-transport.js'

const configPath = process.env.SERVER_WALLET_CONFIG ?? 'config/server-wallet.json'
const walletConfig = JSON.parse(readFileSync(configPath, 'utf-8')) as {
  network: 'testnet' | 'mainnet'
  address: string
  orchardIvk: string
}

const RPC_ENDPOINT = process.env.ZCASH_RPC_ENDPOINT ?? 'https://zcash-testnet-zebrad.gateway.tatum.io'
const MPP_SECRET_KEY = process.env.MPP_SECRET_KEY ?? 'zimppy-mcp-secret-key'
const MCP_SOCKET_PORT = Number(process.env.MCP_SOCKET_PORT ?? 8765)

const listener = createServer((socket) => {
  console.error('[MCP] Client connected')
  console.error(`[MCP]   remote=${socket.remoteAddress ?? 'unknown'}:${socket.remotePort ?? 'unknown'}`)
  const app = createMcpApp({
    walletConfig,
    rpcEndpoint: RPC_ENDPOINT,
    secretKey: MPP_SECRET_KEY,
  })

  const transport = new SocketTransport(socket)
  transport.onerror = (error) => {
    console.error('[MCP] Transport error')
    console.error(error)
  }
  transport.onclose = () => {
    console.error('[MCP] Transport closed')
  }
  app.connect(transport).catch((error) => {
    console.error('[MCP] App connect failed')
    console.error(error)
  })

  socket.on('close', () => {
    console.error('[MCP] Client disconnected')
  })
  socket.on('error', (error) => {
    console.error('[MCP] Socket error')
    console.error(error)
  })
})

listener.listen(MCP_SOCKET_PORT, '127.0.0.1', () => {
  console.error('zimppy MCP server running on socket transport')
  console.error(`  port: ${MCP_SOCKET_PORT}`)
  console.error(`  Server wallet: ${walletConfig.address.slice(0, 20)}...`)
})
