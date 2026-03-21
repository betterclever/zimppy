import { randomUUID } from 'node:crypto'
import { z } from 'zod'
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js'
import { Mcp } from 'mppx'
import { Mppx, Transport } from 'mppx/server'
import { zcashMethod, zcashRequestSchema, zcash } from 'zimppy-ts'

export type WalletConfig = {
  network: 'testnet' | 'mainnet'
  address: string
  orchardIvk: string
}

export function createMcpApp(parameters: {
  walletConfig: WalletConfig
  rpcEndpoint: string
  secretKey: string
}) {
  const { walletConfig, rpcEndpoint, secretKey } = parameters

  console.error('[MCP] Initializing app')
  console.error(`[MCP]   network=${walletConfig.network}`)
  console.error(`[MCP]   recipient=${walletConfig.address}`)
  console.error(`[MCP]   rpc=${rpcEndpoint}`)

  const server = new McpServer({
    name: 'zimppy-mcp',
    version: '0.1.0',
  })

  const payment = Mppx.create({
    methods: [
      zcash({
        orchardIvk: walletConfig.orchardIvk,
        rpcEndpoint,
      }),
    ],
    realm: 'zimppy-mcp',
    secretKey,
    transport: Transport.mcpSdk(),
  })

  const toolPrices: Record<string, string> = {
    get_weather: '42000',
    get_zcash_info: '10000',
  }

  function challengeRequest(
    toolName: keyof typeof toolPrices,
    extra: { _meta?: Record<string, unknown> },
  ) {
    const credential = extra._meta?.[Mcp.credentialMetaKey] as
      | { challenge?: { method?: string; intent?: string; request?: unknown } }
      | undefined

    if (
      credential?.challenge?.method === zcashMethod.name &&
      credential?.challenge?.intent === zcashMethod.intent
    ) {
      const parsed = zcashRequestSchema.parse(credential.challenge.request)
      console.error(`[MCP:${toolName}] Reusing echoed challenge`)
      console.error(`[MCP:${toolName}]   challengeId=${parsed.challengeId}`)
      console.error(`[MCP:${toolName}]   amount=${parsed.amount}`)
      console.error(`[MCP:${toolName}]   memo=${parsed.memo}`)
      return parsed
    }

    const challengeId = randomUUID()
    const fresh = {
      amount: toolPrices[toolName],
      currency: 'ZEC',
      recipient: walletConfig.address,
      network: walletConfig.network,
      memo: `zimppy:${challengeId}`,
      challengeId,
    }
    console.error(`[MCP:${toolName}] Issuing fresh challenge`)
    console.error(`[MCP:${toolName}]   challengeId=${fresh.challengeId}`)
    console.error(`[MCP:${toolName}]   amount=${fresh.amount}`)
    console.error(`[MCP:${toolName}]   memo=${fresh.memo}`)
    return fresh
  }

  function paidTool(
    toolName: keyof typeof toolPrices,
    schema: Record<string, z.ZodType>,
    handler: (args: Record<string, unknown>) => Promise<string>,
  ) {
    server.tool(toolName, schema, async (args, extra) => {
      console.error(`[MCP:${toolName}] Tool invoked`)
      console.error(`[MCP:${toolName}]   args=${JSON.stringify(args)}`)
      const charge = challengeRequest(toolName, extra)
      const result = await payment.charge(charge)(extra)
      if (result.status === 402) {
        console.error(`[MCP:${toolName}] Payment required`)
        throw result.challenge
      }

      console.error(`[MCP:${toolName}] Payment verified`)
      const text = await handler(args as Record<string, unknown>)
      console.error(`[MCP:${toolName}] Tool handler completed`)
      return result.withReceipt({
        content: [{ type: 'text', text }],
      })
    })
  }

  paidTool(
    'get_weather',
    { city: z.string().describe('City name') },
    async (args) => JSON.stringify({
      city: (args.city as string) ?? 'Unknown',
      temperature: Math.floor(Math.random() * 35) + 5,
      condition: ['sunny', 'cloudy', 'rainy', 'windy'][Math.floor(Math.random() * 4)],
      humidity: Math.floor(Math.random() * 60) + 30,
    }, null, 2),
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

  server.tool('ping', {}, async () => {
    console.error('[MCP:ping] Tool invoked')
    return {
      content: [{ type: 'text', text: JSON.stringify({ pong: true, timestamp: new Date().toISOString() }) }],
    }
  })

  return server
}
