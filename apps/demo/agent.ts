#!/usr/bin/env npx tsx
/**
 * Zimppy AI Agent — autonomous tool calling with private Zcash payments.
 *
 * This agent:
 * 1. Takes a user query
 * 2. Decides which MCP tool to call
 * 3. Connects to the paid MCP server
 * 4. Auto-pays with real ZEC when it gets 402
 * 5. Returns the result
 *
 * Usage: npx tsx apps/demo/agent.ts "What's the weather in Tokyo?"
 */

import { Client } from '@modelcontextprotocol/sdk/client/index.js'
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js'
import { McpClient } from 'mppx/mcp-sdk/client'
import { zcashClient } from 'zimppy-ts'
import { createLogger, sendRealPayment } from './autopay.js'

const GREEN = '\x1b[32m'
const YELLOW = '\x1b[33m'
const CYAN = '\x1b[36m'
const RED = '\x1b[31m'
const BOLD = '\x1b[1m'
const NC = '\x1b[0m'

const log = createLogger()

async function main() {
  const query = process.argv[2] ?? 'Tell me about Zcash'

  log(`${BOLD}${CYAN}`)
  log('  ╔════════════════════════════════════════════╗')
  log('  ║  ZIMPPY AI AGENT                           ║')
  log('  ║  Autonomous payments with private Zcash     ║')
  log('  ╚════════════════════════════════════════════╝')
  log(`${NC}`)
  log(`${BOLD}User:${NC} ${query}`)
  log('')

  // Decide which tool to call based on the query
  const tool = decideTool(query)
  log(`${YELLOW}Agent: I'll use the "${tool.name}" tool to answer this.${NC}`)
  log(`${YELLOW}       This tool costs ZEC — I'll pay automatically.${NC}`)
  log('')

  // Connect to MCP server
  log(`${CYAN}[agent] Connecting to MCP server...${NC}`)
  const transport = new StdioClientTransport({
    command: 'npx',
    args: ['tsx', 'apps/mcp-server/src/server.ts'],
    cwd: process.cwd(),
    stderr: 'pipe',
  })

  // Forward MCP server stderr for debugging
  transport.stderr?.on('data', (chunk: Buffer) => {
    const lines = chunk.toString().trim().split('\n')
    for (const line of lines) {
      log(`${CYAN}  [mcp-server] ${line}${NC}`)
    }
  })

  const client = new Client({ name: 'zimppy-agent', version: '0.1.0' })
  await client.connect(transport)

  // Wrap with auto-pay
  const mcp = McpClient.wrap(client, {
    methods: [
      zcashClient({
        async createPayment({ challenge }) {
          log('')
          log(`${RED}[agent] Got 402 Payment Required!${NC}`)
          log(`${YELLOW}[agent] Auto-paying with Zcash...${NC}`)
          log(`${YELLOW}        Amount: ${challenge.amount} zat${NC}`)
          log(`${YELLOW}        To: ${challenge.recipient.slice(0, 25)}...${NC}`)
          log(`${YELLOW}        Memo: ${challenge.memo}${NC}`)
          log('')

          const result = await sendRealPayment(challenge, (line) => {
            if (line) log(`${YELLOW}        ${line}${NC}`)
          })

          log('')
          log(`${GREEN}[agent] Payment confirmed! Retrying tool call...${NC}`)
          log('')
          return result
        },
      }),
    ],
  })

  // Call the tool
  log(`${CYAN}[agent] Calling ${tool.name}(${JSON.stringify(tool.args)})...${NC}`)
  log('')

  try {
    const result = await mcp.callTool({
      name: tool.name,
      arguments: tool.args,
    })

    const content = Array.isArray(result.content)
      ? result.content.map((c: { text?: string }) => c.text ?? JSON.stringify(c)).join('\n')
      : String(result.content)

    log(`${BOLD}${GREEN}Agent:${NC} Here's what I found:`)
    log('')
    log(content)
    log('')

    if (result.receipt) {
      log(`${CYAN}[receipt] method=${result.receipt.method} ref=${result.receipt.reference?.slice(0, 20)}... status=${result.receipt.status}${NC}`)
    }

    log('')
    log(`${BOLD}${GREEN}Payment was fully private — nobody can see this transaction on-chain.${NC}`)
  } catch (err) {
    log(`${RED}Error: ${(err as Error).message}${NC}`)
  }

  await client.close()
  await transport.close()
}

function decideTool(query: string): { name: string; args: Record<string, unknown> } {
  const lower = query.toLowerCase()

  if (lower.includes('weather')) {
    const cityMatch = query.match(/(?:in|for|at)\s+(\w+)/i)
    return { name: 'get_weather', args: { city: cityMatch?.[1] ?? 'Tokyo' } }
  }

  if (lower.includes('zcash') || lower.includes('zec') || lower.includes('crypto') || lower.includes('blockchain')) {
    return { name: 'get_zcash_info', args: {} }
  }

  if (lower.includes('ping') || lower.includes('hello') || lower.includes('test')) {
    return { name: 'ping', args: {} }
  }

  // Default to zcash info for any other query
  return { name: 'get_zcash_info', args: {} }
}

main().catch((err) => {
  console.error(err)
  process.exit(1)
})
