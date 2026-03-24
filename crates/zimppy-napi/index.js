import { createRequire } from 'node:module'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const __dirname = dirname(fileURLToPath(import.meta.url))

const binaryNames = [
  'index.darwin-arm64.node',
  'zimppy-core.darwin-arm64.node',
]

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
  throw new Error(`Failed to load native module. Tried: ${binaryNames.join(', ')}`)
}

const { ZimppyCore, ZimppyWalletNapi } = nativeModule

export { ZimppyCore, ZimppyWalletNapi }
