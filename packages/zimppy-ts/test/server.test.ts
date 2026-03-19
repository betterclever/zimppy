import { describe, it } from 'node:test'
import assert from 'node:assert/strict'
import { ZcashChargeServer } from '../src/server.js'

describe('ZcashChargeServer', () => {
  const server = new ZcashChargeServer({
    recipient: 'tmTestAddr123',
    network: 'testnet',
  })

  it('creates a challenge with correct method and intent', () => {
    const challenge = server.createChallenge('42000')
    assert.equal(challenge.method, 'zcash')
    assert.equal(challenge.intent, 'charge')
    assert.equal(challenge.scheme, 'Payment')
    assert.equal(challenge.request.amount, '42000')
    assert.equal(challenge.request.currency, 'ZEC')
    assert.equal(challenge.request.recipient, 'tmTestAddr123')
    assert.equal(challenge.request.network, 'testnet')
  })

  it('challenge has a UUID challengeId', () => {
    const challenge = server.createChallenge('42000')
    assert.ok(challenge.request.challengeId.length > 0)
  })

  it('challenge expires in the future', () => {
    const challenge = server.createChallenge('42000')
    const expires = new Date(challenge.request.expiresAt)
    assert.ok(expires.getTime() > Date.now())
  })

  it('formats WWW-Authenticate header', () => {
    const challenge = server.createChallenge('42000')
    const header = server.formatWwwAuthenticate(challenge)
    assert.ok(header.startsWith('Payment id="'))
    assert.ok(header.includes('method="zcash"'))
    assert.ok(header.includes('intent="charge"'))
    assert.ok(header.includes('request="'))
  })

  it('parses a valid credential', () => {
    const raw = { payload: { txid: 'deadbeef', outputIndex: 0 } }
    const encoded = Buffer.from(JSON.stringify(raw), 'utf8').toString('base64url')
    const credential = server.parseCredential(`Payment ${encoded}`)
    assert.equal(credential.payload.txid, 'deadbeef')
    assert.equal(credential.payload.outputIndex, 0)
  })

  it('rejects non-Payment scheme', () => {
    assert.throws(() => server.parseCredential('Bearer abc'), /unsupported/)
  })

  it('rejects malformed credential', () => {
    assert.throws(() => server.parseCredential('Payment invalidbase64!!!'))
  })
})
