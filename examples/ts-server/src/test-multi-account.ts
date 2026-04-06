#!/usr/bin/env tsx
/**
 * E2E test for multi-account infrastructure.
 * Creates a fresh temporary wallet (no blockchain sync needed).
 *
 * Usage:
 *   tsx src/test-multi-account.ts
 */
import { createRequire } from 'node:module'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const require = createRequire(import.meta.url)
const { ZimppyWalletNapi: Wallet } = require('@zimppy/core-napi') as typeof import('@zimppy/core-napi')

const LWD = 'https://testnet.zec.rocks'
const NETWORK = 'testnet'

async function main() {
  const tmpDir = mkdtempSync(join(tmpdir(), 'zimppy-test-'))
  console.log('=== Multi-Account E2E Test ===')
  console.log(`  tmpDir: ${tmpDir}\n`)

  try {
    // Create a fresh wallet with 2 accounts
    console.log('[1] Creating fresh wallet with 2 accounts...')
    const wallet = await Wallet.create(
      tmpDir, LWD, NETWORK,
      3_000_000,  // birthday
      null,       // passphrase
      null,       // accountIndex (default 0)
      2,          // numAccounts
    )
    console.log('    Created')

    // numAccounts
    console.log('\n[2] numAccounts...')
    const num = await wallet.numAccounts()
    console.log(`    numAccounts = ${num}`)
    if (num !== 2) throw new Error(`Expected 2, got ${num}`)
    console.log('    PASS')

    // balanceForAccount
    console.log('\n[3] balanceForAccount...')
    for (let i = 0; i < num; i++) {
      const bal = await wallet.balanceForAccount(i)
      console.log(`    Account ${i}: shielded=${bal.totalZat}, transparent=${bal.transparentZat}`)
    }
    console.log('    PASS')

    // generateNextTransparentAddress — should return different addresses
    console.log('\n[4] generateNextTransparentAddress...')
    const addr1 = await wallet.generateNextTransparentAddress()
    console.log(`    Address 1: ${addr1}`)
    const addr2 = await wallet.generateNextTransparentAddress()
    console.log(`    Address 2: ${addr2}`)
    if (addr1 === addr2) throw new Error('Same address returned twice!')
    if (!addr1.startsWith('tm')) throw new Error(`Expected testnet T-addr (tm...), got: ${addr1}`)
    console.log('    Different testnet addresses — PASS')

    // transparentAddresses
    console.log('\n[5] transparentAddresses...')
    const allAddrs = await wallet.transparentAddresses()
    console.log(`    Total: ${allAddrs.length}`)
    if (!allAddrs.includes(addr1) || !allAddrs.includes(addr2)) {
      throw new Error('Generated addresses not in list')
    }
    console.log('    Both found — PASS')

    // transparentAddress (idempotent, index 0)
    console.log('\n[6] transparentAddress (idempotent)...')
    const t1 = await wallet.transparentAddress()
    const t2 = await wallet.transparentAddress()
    if (t1 !== t2) throw new Error(`Not idempotent: ${t1} !== ${t2}`)
    console.log(`    ${t1}`)
    console.log('    Idempotent — PASS')

    // createAccount — add a 3rd account
    console.log('\n[7] createAccount...')
    const newIdx = await wallet.createAccount()
    console.log(`    New account index: ${newIdx}`)
    if (newIdx !== 2) throw new Error(`Expected index 2, got ${newIdx}`)
    const numNow = await wallet.numAccounts()
    if (numNow !== 3) throw new Error(`Expected 3 accounts, got ${numNow}`)
    console.log(`    numAccounts now: ${numNow}`)
    console.log('    PASS')

    // balanceForAccount on new account
    console.log('\n[8] balanceForAccount on new account...')
    const bal2 = await wallet.balanceForAccount(2)
    console.log(`    Account 2: shielded=${bal2.totalZat}, transparent=${bal2.transparentZat}`)
    console.log('    PASS')

    await wallet.close()
    console.log('\n=== All 8 tests passed ===')
  } finally {
    rmSync(tmpDir, { recursive: true, force: true })
  }
}

main().catch((e) => {
  console.error(`\nFAILED: ${(e as Error).message}`)
  console.error(e)
  process.exit(1)
})
