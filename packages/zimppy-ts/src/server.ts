/**
 * zimppy-ts/server — Server-side exports for Zcash MPP payments.
 *
 * Usage:
 *   import { zcash, zcashSession } from 'zimppy-ts/server'
 */

export { zcash, zcashServer, zcashMethod, zcashRequestSchema, zcashCredentialPayloadSchema } from './mppx.js'
export type { ZcashServerOptions, ZcashVerifyResult } from './mppx.js'

export { zcashSession, zcashSessionServer, zcashSessionMethod, sessionRequestSchema, sessionCredentialPayloadSchema } from './session.js'
export type { ZcashSessionServerOptions } from './session.js'

export { NapiCryptoClient, HttpCryptoClient, createCryptoBackend } from './crypto-client.js'
export type { CryptoBackend, ShieldedVerifyResult, VerifyShieldedRequest, VerifyTransparentRequest, VerifyResult } from './crypto-client.js'

export { serveStream, toResponse } from './sse.js'
export type { ServeStreamOptions, NeedTopupEvent, StreamReceipt } from './sse.js'

export {
  SessionNotFoundError,
  SessionClosedError,
  InsufficientBalanceError,
  InvalidBearerError,
  DepositConsumedError,
  DepositVerificationError,
  TopUpConsumedError,
} from './errors.js'
export type { SessionError } from './errors.js'
