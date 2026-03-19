import { describe, it } from 'node:test'
import assert from 'node:assert/strict'
import { ZcashChargeCredential, ZcashChargeRequest, ZCASH_METHOD_NAME } from '../src/index.js'

describe('ZcashChargeCredential schema', () => {
  it('accepts valid credential', () => {
    const result = ZcashChargeCredential.safeParse({
      payload: { txid: 'abc123', outputIndex: 0 },
    })
    assert.ok(result.success)
  })

  it('rejects missing txid', () => {
    const result = ZcashChargeCredential.safeParse({
      payload: { outputIndex: 0 },
    })
    assert.ok(!result.success)
  })

  it('rejects negative outputIndex', () => {
    const result = ZcashChargeCredential.safeParse({
      payload: { txid: 'abc', outputIndex: -1 },
    })
    assert.ok(!result.success)
  })
})

describe('ZcashChargeRequest schema', () => {
  it('accepts valid request', () => {
    const result = ZcashChargeRequest.safeParse({
      amount: '42000',
      currency: 'ZEC',
      recipient: 'tmTestAddr',
      network: 'testnet',
    })
    assert.ok(result.success)
  })

  it('rejects non-ZEC currency', () => {
    const result = ZcashChargeRequest.safeParse({
      amount: '42000',
      currency: 'BTC',
      recipient: 'tmTestAddr',
      network: 'testnet',
    })
    assert.ok(!result.success)
  })

  it('rejects invalid network', () => {
    const result = ZcashChargeRequest.safeParse({
      amount: '42000',
      currency: 'ZEC',
      recipient: 'tmTestAddr',
      network: 'devnet',
    })
    assert.ok(!result.success)
  })
})

describe('method constants', () => {
  it('method name is zcash', () => {
    assert.equal(ZCASH_METHOD_NAME, 'zcash')
  })
})
