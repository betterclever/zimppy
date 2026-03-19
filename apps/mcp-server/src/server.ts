import { randomUUID } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { z } from 'zod'
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js'
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js'
import { Mppx, Transport } from 'mppx/server'
import { zcashServer } from 'zimppy-ts'

const configPath = process.env.SERVER_WALLET_CONFIG ?? 'config/server-wallet.json'
const walletConfig = JSON.parse(readFileSync(configPath, 'utf-8')) as {
  network: 'testnet' | 'mainnet'
  address: string
  orchardIvk: string
}

const RPC_ENDPOINT = process.env.ZCASH_RPC_ENDPOINT ?? 'https://zcash-testnet-zebrad.gateway.tatum.io'
const MPP_SECRET_KEY = process.env.MPP_SECRET_KEY ?? 'zimppy-mcp-secret-key'

const server = new McpServer({
  name: 'zimppy-mcp',
  version: '0.1.0',
})

const payment = Mppx.create({
  methods: [
    zcashServer({
      orchardIvk: walletConfig.orchardIvk,
      rpcEndpoint: RPC_ENDPOINT,
    }),
  ],
  realm: 'zimppy-mcp',
  secretKey: MPP_SECRET_KEY,
  transport: Transport.mcpSdk(),
})

const TOOL_PRICES: Record<string, string> = {
  get_weather: '42000',
  get_zcash_info: '10000',
}

function challengeRequest(toolName: keyof typeof TOOL_PRICES) {
  const challengeId = randomUUID()
  return {
    amount: TOOL_PRICES[toolName],
    currency: 'ZEC',
    recipient: walletConfig.address,
    network: walletConfig.network,
    memo: `zimppy:${challengeId}`,
    challengeId,
  }
}

function paidTool(
  toolName: keyof typeof TOOL_PRICES,
  schema: Record<string, z.ZodType>,
  handler: (args: Record<string, unknown>) => Promise<string>,
) {
  server.tool(toolName, schema, async (args, extra) => {
    const result = await payment.charge(challengeRequest(toolName))(extra)
    if (result.status === 402) {
      console.error(`[MCP:${toolName}] Payment required`)
      throw result.challenge
    }

    console.error(`[MCP:${toolName}] Payment verified`)
    return result.withReceipt({
      content: [{ type: 'text', text: await handler(args as Record<string, unknown>) }],
    })
  })
}

paidTool(
  'get_weather',
  { city: z.string().describe('City name') },
  async (args) => {
    const city = (args.city as string) ?? 'Unknown'
    const weather = {
      city,
      temperature: Math.floor(Math.random() * 35) + 5,
      condition: ['sunny', 'cloudy', 'rainy', 'windy'][Math.floor(Math.random() * 4)],
      humidity: Math.floor(Math.random() * 60) + 30,
    }
    return JSON.stringify(weather, null, 2)
  },
)

paidTool(
  'get_zcash_info',
  {},
  async () => JSON.stringify({
    network: walletConfig.network,
    protocol: 'Zcash',
    pools: ['transparent', 'sapling', 'orchard'],
    blockTime: '~75 seconds',
    consensus: 'Proof-of-Work (Equihash)',
    privacyFeature: 'zk-SNARKs (Halo 2)',
    serverAddress: walletConfig.address,
  }, null, 2),
)

server.tool('ping', {}, async () => ({
  content: [{ type: 'text', text: JSON.stringify({ pong: true, timestamp: new Date().toISOString() }) }],
}))

async function main() {
  const transport = new StdioServerTransport()
  await server.connect(transport)
  console.error('zimppy MCP server running on stdio')
  console.error(`  Server wallet: ${walletConfig.address.slice(0, 20)}...`)
  console.error(`  Network: ${walletConfig.network}`)
  console.error(`  RPC: ${RPC_ENDPOINT}`)
  console.error(`  Paid tools: ${Object.keys(TOOL_PRICES).join(', ')}`)
}

main().catch(console.error)
