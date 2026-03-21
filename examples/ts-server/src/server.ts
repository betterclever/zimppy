import { randomUUID } from 'node:crypto'
import { createServer } from 'node:http'
import { readFileSync } from 'node:fs'
import { Credential } from 'mppx'
import { Mppx, NodeListener, Request as MppRequest } from 'mppx/server'
import { zcashMethod, zcashRequestSchema, zcash } from 'zimppy-ts'

const configPath = process.env.SERVER_WALLET_CONFIG ?? 'config/server-wallet.json'
const walletConfig = JSON.parse(readFileSync(configPath, 'utf-8')) as {
  network: 'testnet' | 'mainnet'
  address: string
  orchardIvk: string
}

const RPC_ENDPOINT = process.env.ZCASH_RPC_ENDPOINT ?? 'https://zcash-testnet-zebrad.gateway.tatum.io'
const MPP_SECRET_KEY = process.env.MPP_SECRET_KEY ?? 'zimppy-ts-http-secret-key'
const PORT = Number(process.env.PORT ?? 3180)
const PRICE_ZAT = String(process.env.PRICE_ZAT ?? '42000')

const payment = Mppx.create({
  methods: [
    zcash({
      orchardIvk: walletConfig.orchardIvk,
      rpcEndpoint: RPC_ENDPOINT,
    }),
  ],
  realm: 'zimppy-ts-http',
  secretKey: MPP_SECRET_KEY,
})

const handlePaidFortune = async (request: Request) => {
  const challengeRequest = getChargeRequest(request)
  const result = await payment.charge({
    ...challengeRequest,
  })(request)

  if (result.status === 402) {
    return result.challenge
  }

  const fortune = pickFortune()
  console.error(`[TS-HTTP] Payment verified. Fortune: ${fortune}`)
  return result.withReceipt(
    Response.json({ fortune }),
  )
}

function getChargeRequest(request: Request) {
  const authorization = request.headers.get('Authorization')
  const paymentHeader = authorization ? Credential.extractPaymentScheme(authorization) : null

  if (paymentHeader) {
    try {
      const credential = Credential.deserialize(paymentHeader)
      if (
        credential.challenge.method === zcashMethod.name &&
        credential.challenge.intent === zcashMethod.intent
      ) {
        return zcashRequestSchema.parse(credential.challenge.request)
      }
    } catch {
      // Let mppx handle malformed credentials through the normal challenge flow.
    }
  }

  const challengeId = randomUUID()
  return {
    amount: PRICE_ZAT,
    currency: 'ZEC',
    recipient: walletConfig.address,
    network: walletConfig.network,
    memo: `zimppy:${challengeId}`,
    challengeId,
  }
}

const server = createServer(async (req, res) => {
  const url = new URL(req.url ?? '/', `http://${req.headers.host ?? '127.0.0.1'}`)

  if (url.pathname === '/api/health') {
    await NodeListener.sendResponse(
      res,
      Response.json({ status: 'ok', service: 'zimppy-ts-http-server' }),
    )
    return
  }

  // Non-standard convenience endpoint (MPP discovery spec uses /openapi.json)
  if (url.pathname === '/.well-known/payment') {
    await NodeListener.sendResponse(
      res,
      Response.json({
        methods: ['zcash'],
        intents: ['charge'],
        network: walletConfig.network,
        recipient: walletConfig.address,
        defaultAmount: PRICE_ZAT,
        currency: 'ZEC',
        memo_format: 'zimppy:{challenge_id}',
      }),
    )
    return
  }

  if (url.pathname === '/api/fortune') {
    const request = MppRequest.fromNodeListener(req, res)
    const response = await handlePaidFortune(request)
    await NodeListener.sendResponse(res, response)
    return
  }

  await NodeListener.sendResponse(
    res,
    new Response('Not Found', { status: 404 }),
  )
})

server.listen(PORT, '0.0.0.0', () => {
  console.error('=== zimppy TS HTTP server ===')
  console.error(`  network: ${walletConfig.network}`)
  console.error(`  address: ${walletConfig.address.slice(0, 20)}...`)
  console.error(`  price: ${PRICE_ZAT} zat per request`)
  console.error(`  RPC: ${RPC_ENDPOINT}`)
  console.error(`  port: ${PORT}`)
  console.error(`  discovery: http://127.0.0.1:${PORT}/.well-known/payment`)
})

function pickFortune(): string {
  const fortunes = [
    'Privacy is not about having something to hide.',
    'The best shield is the one nobody knows about.',
    'Zero knowledge, full power.',
    'A shielded transaction brings peace of mind.',
    'Trust in math, not middlemen.',
  ]

  const idx = Math.floor(Date.now() / 1000) % fortunes.length
  return fortunes[idx]!
}
