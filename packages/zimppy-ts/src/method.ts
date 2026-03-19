import { z } from 'zod'

export const ZcashChargeCredential = z.object({
  payload: z.object({
    txid: z.string().min(1),
    outputIndex: z.number().int().min(0),
  }),
})

export const ZcashChargeRequest = z.object({
  amount: z.string().min(1),      // zatoshis as string
  currency: z.literal('ZEC'),
  recipient: z.string().min(1),
  network: z.enum(['testnet', 'mainnet']),
})

export type ZcashCredential = z.infer<typeof ZcashChargeCredential>
export type ZcashRequest = z.infer<typeof ZcashChargeRequest>

export const ZCASH_METHOD_NAME = 'zcash'
export const ZCASH_CHARGE_INTENT = 'charge'
