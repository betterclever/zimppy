# zimppy-ts

[![npm](https://img.shields.io/npm/v/zimppy-ts)](https://www.npmjs.com/package/zimppy-ts)

TypeScript SDK for [zimppy](https://zimppy.xyz) — the privacy stack for [MPP](https://mpp.dev).

Charge, session, and SSE streaming payments over Zcash.

## Server

```ts
import { Mppx } from 'mppx/server'
import { zcash } from 'zimppy-ts/server'

const mppx = Mppx.create({
  methods: [await zcash({ wallet: 'server' })],
  realm: 'my-api',
  secretKey: process.env.MPP_SECRET_KEY,
})

const result = await mppx.charge({ amount: '42000', currency: 'zec' })(request)
if (result.status === 402) return result.challenge
return result.withReceipt(Response.json({ data }))
```

## Client

```ts
import { Mppx } from 'mppx/client'
import { zcash } from 'zimppy-ts/client'

const mppx = Mppx.create({ methods: [zcash({ wallet: 'default' })] })
const res = await mppx.fetch('https://api.example.com/resource')
```

## Install

```bash
npm install zimppy-ts
```

## License

MIT
