import { createRequire } from 'node:module'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const __dirname = dirname(fileURLToPath(import.meta.url))

const { ZimppyCore, ZimppyWalletNapi } = require(join(__dirname, 'zimppy-core.darwin-arm64.node'))

export { ZimppyCore, ZimppyWalletNapi }
