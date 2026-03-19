import { stdin as input, stdout as output } from 'node:process'
import { createInterface } from 'node:readline/promises'
import { Client } from '@modelcontextprotocol/sdk/client/index.js'
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js'
import { McpClient } from 'mppx/mcp-sdk/client'
import { zcashClient } from 'zimppy-ts'
import { SocketTransport } from './socket-transport.js'
import { createLogger, sendRealPayment } from './autopay.js'

const log = createLogger()

async function main(): Promise<void> {
  log('=== Zcash MCP REPL Demo ===')

  const transport = createTransport()
  const client = new Client({ name: 'zimppy-mcp-repl', version: '0.1.0' })
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

  const rl = createInterface({ input, output })

  printHelp()
  const autorun = (process.env.MCP_DEMO_AUTORUN ?? '').trim()
  if (autorun) {
    await handleCommand(mcp as any, autorun)
  }

  while (true) {
    const line = (await rl.question('mcp> ')).trim()
    if (!line) {
      continue
    }

    if (line === 'quit' || line === 'exit') {
      break
    }

    await handleCommand(mcp as any, line)
  }

  rl.close()
  await client.close()
  await transport.close()
}

async function handleCommand(
  mcp: { callTool: (params: { name: string; arguments?: Record<string, unknown> }) => Promise<any> },
  line: string,
) {
  if (line === 'help') {
    printHelp()
    return
  }

  if (line === 'ping') {
    await runTool(mcp, 'ping', {})
    return
  }

  if (line === 'info') {
    await runTool(mcp, 'get_zcash_info', {})
    return
  }

  if (line.startsWith('weather ')) {
    const city = line.slice('weather '.length).trim()
    if (!city) {
      console.log('usage: weather <city>')
      return
    }
    await runTool(mcp, 'get_weather', { city })
    return
  }

  console.log(`unknown command: ${line}`)
}

async function runTool(
  mcp: { callTool: (params: { name: string; arguments?: Record<string, unknown> }) => Promise<any> },
  name: string,
  args: Record<string, unknown>,
) {
  console.log(`→ ${name}`)
  const result = await mcp.callTool({ name, arguments: args })
  const content = Array.isArray(result.content) ? result.content : []
  for (const entry of content) {
    if ('text' in entry) {
      console.log(entry.text)
    } else {
      console.log(JSON.stringify(entry))
    }
  }
  if (result.receipt) {
    console.log(`receipt.reference=${result.receipt.reference}`)
    console.log(`receipt.challengeId=${result.receipt.challengeId}`)
  }
}

function createTransport() {
  if (false) { // removed FIFO path
    throw new Error("unreachable")
  }

  if (process.env.MCP_SOCKET_PORT) {
    return SocketTransport.connect(Number(process.env.MCP_SOCKET_PORT), process.env.MCP_SOCKET_HOST ?? '127.0.0.1')
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

function printHelp() {
  console.log('Commands:')
  console.log('  ping')
  console.log('  info')
  console.log('  weather <city>')
  console.log('  help')
  console.log('  quit')
}

main().catch(async (error: unknown) => {
  console.error(error instanceof Error ? error.stack ?? error.message : String(error))
  process.exitCode = 1
})
