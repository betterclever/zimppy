import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js'
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js'
import { ZcashChargeServer } from 'zimppy-ts'

const RECIPIENT = process.env.ZCASH_RECIPIENT ?? 'tmHQEhKoEkBFR49E6dGG1QCMz4VEBrTpjCp'
const NETWORK = 'testnet' as const
const CRYPTO_ENDPOINT = process.env.CRYPTO_ENDPOINT ?? 'http://127.0.0.1:3181'

const zcash = new ZcashChargeServer({
  recipient: RECIPIENT,
  network: NETWORK,
  cryptoEndpoint: CRYPTO_ENDPOINT,
})

const server = new McpServer({
  name: 'zimppy-mcp',
  version: '0.1.0',
})

// Tool pricing in zatoshis
const TOOL_PRICES: Record<string, string> = {
  get_weather: '42000',      // 0.00042 ZEC
  get_zcash_info: '10000',   // 0.0001 ZEC
}

server.tool(
  'get_weather',
  'Get weather for a city (costs 42000 zatoshis / 0.00042 ZEC)',
  { city: { type: 'string', description: 'City name' } },
  async (args, extra) => {
    const meta = extra._meta as Record<string, unknown> | undefined
    const credential = meta?.['org.paymentauth/credential'] as string | undefined

    if (!credential) {
      const challenge = zcash.createChallenge(TOOL_PRICES.get_weather)
      return {
        content: [{ type: 'text', text: 'Payment required' }],
        _meta: {
          'org.paymentauth/challenge': challenge,
          'org.paymentauth/www-authenticate': zcash.formatWwwAuthenticate(challenge),
        },
        isError: true,
      }
    }

    try {
      const parsed = zcash.parseCredential(credential)
      const receipt = await zcash.verify(parsed, TOOL_PRICES.get_weather)

      const city = (args as Record<string, string>).city ?? 'Unknown'
      // Mock weather data
      const weather = {
        city,
        temperature: Math.floor(Math.random() * 35) + 5,
        condition: ['sunny', 'cloudy', 'rainy', 'windy'][Math.floor(Math.random() * 4)],
        humidity: Math.floor(Math.random() * 60) + 30,
      }

      return {
        content: [{ type: 'text', text: JSON.stringify(weather, null, 2) }],
        _meta: { 'org.paymentauth/receipt': receipt },
      }
    } catch (err) {
      return {
        content: [{ type: 'text', text: `Payment verification failed: ${(err as Error).message}` }],
        isError: true,
      }
    }
  },
)

server.tool(
  'get_zcash_info',
  'Get Zcash network info (costs 10000 zatoshis / 0.0001 ZEC)',
  {},
  async (_args, extra) => {
    const meta = extra._meta as Record<string, unknown> | undefined
    const credential = meta?.['org.paymentauth/credential'] as string | undefined

    if (!credential) {
      const challenge = zcash.createChallenge(TOOL_PRICES.get_zcash_info)
      return {
        content: [{ type: 'text', text: 'Payment required' }],
        _meta: {
          'org.paymentauth/challenge': challenge,
          'org.paymentauth/www-authenticate': zcash.formatWwwAuthenticate(challenge),
        },
        isError: true,
      }
    }

    try {
      const parsed = zcash.parseCredential(credential)
      const receipt = await zcash.verify(parsed, TOOL_PRICES.get_zcash_info)

      const info = {
        network: 'testnet',
        protocol: 'Zcash',
        pools: ['transparent', 'sapling', 'orchard'],
        blockTime: '~75 seconds',
        consensus: 'Proof-of-Work (Equihash)',
        privacyFeature: 'zk-SNARKs (Halo 2)',
      }

      return {
        content: [{ type: 'text', text: JSON.stringify(info, null, 2) }],
        _meta: { 'org.paymentauth/receipt': receipt },
      }
    } catch (err) {
      return {
        content: [{ type: 'text', text: `Payment verification failed: ${(err as Error).message}` }],
        isError: true,
      }
    }
  },
)

// Free tool for testing
server.tool(
  'ping',
  'Free ping tool for testing connectivity',
  {},
  async () => ({
    content: [{ type: 'text', text: JSON.stringify({ pong: true, timestamp: new Date().toISOString() }) }],
  }),
)

async function main() {
  const transport = new StdioServerTransport()
  await server.connect(transport)
  console.error('zimppy MCP server running on stdio')
  console.error(`  Recipient: ${RECIPIENT}`)
  console.error(`  Network: ${NETWORK}`)
  console.error(`  Crypto endpoint: ${CRYPTO_ENDPOINT}`)
}

main().catch(console.error)
