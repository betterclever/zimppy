import { createRequire } from 'node:module'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const __dirname = dirname(fileURLToPath(import.meta.url))

const binaryNamesByTarget = {
  'darwin-arm64': [
    'zimppy-core.darwin-arm64.node',
    'index.darwin-arm64.node',
  ],
  'linux-x64': [
    'zimppy-core.linux-x64-gnu.node',
    'index.linux-x64-gnu.node',
  ],
}

const target = `${process.platform}-${process.arch}`
const binaryNames = binaryNamesByTarget[target] ?? []

let nativeModule = null
for (const binaryName of binaryNames) {
  try {
    nativeModule = require(join(__dirname, binaryName))
    break
  } catch {
    // try the next known binary name
  }
}

if (!nativeModule) {
  const tried = binaryNames.length > 0 ? binaryNames.join(', ') : '(no supported binaries for this platform)'
  throw new Error(`Failed to load native module for ${target}. Tried: ${tried}`)
}

const { ZimppyCore, ZimppyWalletNapi } = nativeModule

export { ZimppyCore, ZimppyWalletNapi }
