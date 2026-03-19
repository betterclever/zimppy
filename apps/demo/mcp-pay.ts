import { Client } from '@modelcontextprotocol/sdk/client/index.js'
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js'
import { McpClient } from 'mppx/mcp-sdk/client'
import { zcashClient } from 'zimppy-ts'
import { SocketTransport } from './socket-transport.js'
import { createLogger, sendRealPayment } from './autopay.js'

const log = createLogger()

async function main(): Promise<void> {
  log('=== Zcash MCP Auto-Pay Demo ===')
  const transport = createTransport()

  const client = new Client({ name: 'zimppy-mcp-demo', version: '0.1.0' })
  await client.connect(transport)

  const mcp = McpClient.wrap(client, {
    methods: [
      zcashClient({
        async createPayment({ challenge }) {
          return await sendRealPayment(challenge, log)
        },
      }),
    ],
  })

  log('Step 1: Calling paid MCP tool...')
  const result = await mcp.callTool({
    name: 'get_zcash_info',
    arguments: {},
  })

  log('Step 2: MCP tool completed')
  const content = Array.isArray(result.content) ? result.content : []
  log(`  Content: ${content.map((entry: { text?: string }) => entry.text ?? JSON.stringify(entry)).join('\n')}`)
  if (result.receipt) {
    log(`  Receipt reference: ${result.receipt.reference}`)
    log(`  Receipt challengeId: ${result.receipt.challengeId}`)
    log(`  Receipt timestamp: ${result.receipt.timestamp}`)
  } else {
    throw new Error('missing MCP payment receipt')
  }

  await client.close()
  await transport.close()
  log('=== MCP auto-pay demo complete ===')
}

function createTransport() {
  if (process.env.MCP_SOCKET_PORT) {
    return SocketTransport.connect(Number(process.env.MCP_SOCKET_PORT), process.env.MCP_SOCKET_HOST ?? "127.0.0.1")
  }

  const transport = new StdioClientTransport({
    command: 'npx',
    args: ['tsx', 'apps/mcp-server/src/server.ts'],
    cwd: '/Users/betterclever/newprojects/experiments/zimppy',
    stderr: 'pipe',
  })

  const stderr = transport.stderr
  if (stderr) {
    stderr.on('data', (chunk) => {
      process.stderr.write(String(chunk))
    })
  }

  return transport
}

main().catch(async (error: unknown) => {
  console.error(error instanceof Error ? error.stack ?? error.message : String(error))
  process.exitCode = 1
})
