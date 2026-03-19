import { describe, it } from 'node:test'
import assert from 'node:assert/strict'
import { Challenge, Credential, Receipt } from 'mppx'

import { NapiCryptoClient } from '../src/crypto-client.js'
import {
  zcashClient,
  zcashCredentialPayloadSchema,
  zcashMethod,
  zcashRequestSchema,
  zcashServer,
} from '../src/index.js'

describe('zcash mppx method', () => {
  it('defines typed request and credential payload schemas', () => {
    const request = zcashRequestSchema.parse({
      amount: '42000',
      currency: 'ZEC',
      recipient: 'utest1example',
      network: 'testnet',
      memo: 'zimppy:challenge-123',
      challengeId: 'challenge-123',
    })

    const payload = zcashCredentialPayloadSchema.parse({
      txid: 'deadbeef',
      challengeId: 'challenge-123',
    })

    assert.equal(request.challengeId, 'challenge-123')
    assert.equal(payload.txid, 'deadbeef')
    assert.equal(zcashMethod.name, 'zcash')
    assert.equal(zcashMethod.intent, 'charge')
  })
})

describe('zcashServer', () => {
  it('verifies via injected verifier and returns an mppx receipt', async () => {
    const server = zcashServer({
      verifyPayment: async ({ amount, challengeId, txid }) => {
        assert.equal(amount, '42000')
        assert.equal(challengeId, 'challenge-123')
        assert.equal(txid, 'tx-123')
        return { verified: true, txid, reference: 'verified-ref' }
      },
    })

    const challenge = Challenge.fromMethod(zcashMethod, {
      id: 'challenge-123',
      realm: 'zimppy',
      request: {
        amount: '42000',
        currency: 'ZEC',
        recipient: 'utest1example',
        network: 'testnet',
        memo: 'zimppy:challenge-123',
        challengeId: 'challenge-123',
      },
    })

    const credential = Credential.from({
      challenge,
      payload: {
        txid: 'tx-123',
      },
    })

    const receipt = await server.verify({
      credential: credential as Parameters<typeof server.verify>[0]['credential'],
      request: challenge.request,
    })

    assert.deepEqual(receipt, Receipt.from({
      method: 'zcash',
      status: 'success',
      timestamp: receipt.timestamp,
      reference: 'verified-ref',
    }))
  })

  it('uses the shielded NAPI verifier by default', async () => {
    const originalVerifyShielded = NapiCryptoClient.prototype.verifyShielded
    NapiCryptoClient.prototype.verifyShielded = async function(req) {
      assert.equal(req.txid, 'tx-native')
      assert.equal(req.expectedChallengeId, 'challenge-native')
      assert.equal(req.expectedAmountZat, '55000')
      assert.equal(req.orchardIvk, 'orchard-ivk')
      return {
        verified: true,
        txid: 'tx-native',
        observedAmountZat: '55000',
        memoMatched: true,
        outputsDecrypted: 1,
      }
    }

    try {
      const server = zcashServer({
        rpcEndpoint: 'https://zcash-testnet-zebrad.gateway.tatum.io',
        orchardIvk: 'orchard-ivk',
      })

      const challenge = Challenge.fromMethod(zcashMethod, {
        id: 'challenge-native',
        realm: 'zimppy',
        request: {
          amount: '55000',
          currency: 'ZEC',
          recipient: 'utest1example',
          network: 'testnet',
          memo: 'zimppy:challenge-native',
          challengeId: 'challenge-native',
        },
      })

      const credential = Credential.from({
        challenge,
        payload: {
          txid: 'tx-native',
        },
      })

      const receipt = await server.verify({
        credential: credential as Parameters<typeof server.verify>[0]['credential'],
        request: challenge.request,
      })

      assert.equal(receipt.reference, 'tx-native')
    } finally {
      NapiCryptoClient.prototype.verifyShielded = originalVerifyShielded
    }
  })
})

describe('zcashClient', () => {
  it('serializes a credential with the echoed challenge and txid', async () => {
    const client = zcashClient({
      source: 'did:key:test-client',
      createPayment: async ({ challenge }) => {
        assert.equal(challenge.challengeId, 'challenge-client')
        return {
          txid: 'tx-client',
        }
      },
    })

    const challenge = Challenge.fromMethod(zcashMethod, {
      id: 'challenge-client',
      realm: 'zimppy',
      request: {
        amount: '75000',
        currency: 'ZEC',
        recipient: 'utest1example',
        network: 'testnet',
        memo: 'zimppy:challenge-client',
        challengeId: 'challenge-client',
      },
    })

    const serialized = await client.createCredential({ challenge: challenge as Parameters<typeof client.createCredential>[0]['challenge'] })
    const parsed = Credential.deserialize<{ txid: string }>(serialized)

    assert.equal(parsed.challenge.id, 'challenge-client')
    assert.equal(parsed.payload.txid, 'tx-client')
    assert.equal(parsed.source, 'did:key:test-client')
  })

  it('fails clearly when auto-pay is not configured', async () => {
    const client = zcashClient()
    const challenge = Challenge.fromMethod(zcashMethod, {
      id: 'challenge-missing',
      realm: 'zimppy',
      request: {
        amount: '10000',
        currency: 'ZEC',
        recipient: 'utest1example',
        network: 'testnet',
        memo: 'zimppy:challenge-missing',
        challengeId: 'challenge-missing',
      },
    })

    await assert.rejects(
      () => client.createCredential({ challenge: challenge as Parameters<typeof client.createCredential>[0]['challenge'] }),
      /auto-pay is not configured/,
    )
  })

  it('accepts a challenge cast from mppx Challenge.fromMethod typing', async () => {
    const client = zcashClient({
      createPayment: async () => ({ txid: 'tx-cast' }),
    })
    const challenge = Challenge.fromMethod(zcashMethod, {
      id: 'challenge-cast',
      realm: 'zimppy',
      request: {
        amount: '10000',
        currency: 'ZEC',
        recipient: 'utest1example',
        network: 'testnet',
        memo: 'zimppy:challenge-cast',
        challengeId: 'challenge-cast',
      },
    })

    const serialized = await client.createCredential({
      challenge: challenge as Parameters<typeof client.createCredential>[0]['challenge'],
    })

    const parsed = Credential.deserialize<{ txid: string }>(serialized)
    assert.equal(parsed.payload.txid, 'tx-cast')
  })
})
