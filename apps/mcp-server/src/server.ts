import { randomUUID } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js'
import { createMcpApp } from './app.js'

const configPath = process.env.SERVER_WALLET_CONFIG ?? 'config/server-wallet.json'
const walletConfig = JSON.parse(readFileSync(configPath, 'utf-8')) as {
  network: 'testnet' | 'mainnet'
  address: string
  orchardIvk: string
}

const RPC_ENDPOINT = process.env.ZCASH_RPC_ENDPOINT ?? 'https://zcash-testnet-zebrad.gateway.tatum.io'
const MPP_SECRET_KEY = process.env.MPP_SECRET_KEY ?? 'zimppy-mcp-secret-key'

const server = createMcpApp({
  walletConfig,
  rpcEndpoint: RPC_ENDPOINT,
  secretKey: MPP_SECRET_KEY,
})

async function main() {
  const transport = new StdioServerTransport()
  await server.connect(transport)
  console.error('zimppy MCP server running on stdio')
  console.error(`  Server wallet: ${walletConfig.address.slice(0, 20)}...`)
  console.error(`  Network: ${walletConfig.network}`)
  console.error(`  RPC: ${RPC_ENDPOINT}`)
  console.error('  Paid tools: get_weather, get_zcash_info')
}

main().catch(console.error)
