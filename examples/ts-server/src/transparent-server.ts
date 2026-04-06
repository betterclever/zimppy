/**
 * E2E test server for zcashtransparent payment method.
 *
 * Usage:
 *   tsx src/transparent-server.ts
 *
 * Then hit: curl http://localhost:3181/api/fortune
 */
import { createServer } from 'node:http'
import { Mppx, NodeListener, Request as MppRequest } from 'mppx/server'
import { zcashTransparent } from 'zimppy-ts/server'

const RPC_ENDPOINT = process.env.ZCASH_RPC_ENDPOINT ?? 'https://zcash-testnet-zebrad.gateway.tatum.io'
const MPP_SECRET_KEY = process.env.MPP_SECRET_KEY ?? 'zimppy-ts-transparent-test-key'
const PORT = Number(process.env.PORT ?? 3181)
const PRICE_ZAT = String(process.env.PRICE_ZAT ?? '100000')
const WALLET = process.env.WALLET ?? 'default'

const transparentMethod = await zcashTransparent({
  wallet: WALLET,
  rpcEndpoint: RPC_ENDPOINT,
})

const payment = Mppx.create({
  methods: [transparentMethod],
  realm: 'zimppy-transparent-test',
  secretKey: MPP_SECRET_KEY,
})

const server = createServer(async (req, res) => {
  const url = new URL(req.url ?? '/', `http://${req.headers.host ?? '127.0.0.1'}`)

  if (url.pathname === '/api/health') {
    await NodeListener.sendResponse(res, Response.json({ status: 'ok', method: 'zcashtransparent' }))
    return
  }

  if (url.pathname === '/api/fortune') {
    const request = MppRequest.fromNodeListener(req, res)
    const result = await payment.charge({
      amount: PRICE_ZAT,
      currency: 'zec',
    })(request)

    if (result.status === 402) {
      await NodeListener.sendResponse(res, result.challenge)
      return
    }

    const fortune = pickFortune()
    console.error(`[transparent-server] Payment verified. Fortune: ${fortune}`)
    await NodeListener.sendResponse(res, result.withReceipt(Response.json({ fortune })))
    return
  }

  await NodeListener.sendResponse(res, new Response('Not Found', { status: 404 }))
})

server.listen(PORT, '0.0.0.0', () => {
  console.error('=== zimppy transparent payment test server ===')
  console.error(`  method:  zcashtransparent`)
  console.error(`  wallet:  ${WALLET}`)
  console.error(`  price:   ${PRICE_ZAT} zat per request`)
  console.error(`  RPC:     ${RPC_ENDPOINT}`)
  console.error(`  port:    ${PORT}`)
  console.error(`  health:  http://127.0.0.1:${PORT}/api/health`)
  console.error(`  fortune: http://127.0.0.1:${PORT}/api/fortune`)
})

function pickFortune(): string {
  const fortunes = [
    'Transparent payments: visible on-chain, private in intent.',
    'Every t-address tells a story.',
    'ZEC: shielded when you want, transparent when you need.',
    'Speed and simplicity, with ZEC.',
    'The chain sees the amount. Only you know the reason.',
  ]
  return fortunes[Math.floor(Date.now() / 1000) % fortunes.length]!
}
