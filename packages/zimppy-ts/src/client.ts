/**
 * zimppy-ts/client — Client-side exports for Zcash MPP payments.
 *
 * Usage:
 *   import { zcashClient, zcashSessionClient } from 'zimppy-ts/client'
 */

export { zcashClient, zcashMethod, zcashRequestSchema, zcashCredentialPayloadSchema } from './mppx.js'
export type { ZcashClientOptions, ZcashClientPayment } from './mppx.js'

export { zcashSessionClient, zcashSessionMethod, sessionRequestSchema, sessionCredentialPayloadSchema } from './session.js'
export type { ZcashSessionClientOptions } from './session.js'

export { parseEvent, isEventStream, iterateData } from './sse.js'
export type { SseEvent, NeedTopupEvent, StreamReceipt } from './sse.js'
