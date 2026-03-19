import { ZcashChargeServer } from 'zimppy-ts'

const SERVER_URL = process.env.SERVER_URL ?? 'http://127.0.0.1:3180'
const RECIPIENT = process.env.ZCASH_RECIPIENT ?? 'tmHQEhKoEkBFR49E6dGG1QCMz4VEBrTpjCp'

async function main() {
  console.log('=== Zcash MPP Demo ===')
  console.log(`Server: ${SERVER_URL}`)
  console.log(`Recipient: ${RECIPIENT}`)
  console.log()

  // Step 1: Request a paid resource without payment
  console.log('Step 1: Requesting paid resource without payment...')
  const resp1 = await fetch(`${SERVER_URL}/api/fortune`)
  console.log(`  Status: ${resp1.status}`)

  if (resp1.status === 402) {
    const wwwAuth = resp1.headers.get('www-authenticate')
    console.log(`  WWW-Authenticate: ${wwwAuth?.slice(0, 80)}...`)
    console.log()
    console.log('  Payment required! Challenge received.')
    console.log()

    // Step 2: In a real flow, the client would:
    //   a) Parse the challenge to get recipient, amount
    //   b) Send ZEC on-chain to the recipient
    //   c) Get the txid
    //   d) Construct a credential with the txid
    //   e) Retry the request with the credential

    console.log('Step 2: To complete the payment flow:')
    console.log('  1. Send ZEC to the recipient address shown in the challenge')
    console.log('  2. Get the transaction ID (txid)')
    console.log('  3. Construct a credential:')
    console.log('     { "payload": { "txid": "<your-txid>", "outputIndex": 0 } }')
    console.log('  4. Base64url-encode the credential')
    console.log('  5. Retry with Authorization: Payment <encoded-credential>')
    console.log()

    // Step 3: Simulate with a mock txid (will fail verification but shows the flow)
    console.log('Step 3: Simulating credential submission (will fail - no real tx)...')
    const mockCredential = { payload: { txid: '0'.repeat(64), outputIndex: 0 } }
    const encoded = Buffer.from(JSON.stringify(mockCredential), 'utf8').toString('base64url')

    const resp2 = await fetch(`${SERVER_URL}/api/fortune`, {
      headers: { Authorization: `Payment ${encoded}` },
    })
    console.log(`  Status: ${resp2.status}`)
    const body = await resp2.text()
    console.log(`  Body: ${body.slice(0, 200)}`)
  } else {
    const body = await resp1.text()
    console.log(`  Unexpected response: ${body}`)
  }

  console.log()
  console.log('=== Demo Complete ===')
}

main().catch(console.error)
