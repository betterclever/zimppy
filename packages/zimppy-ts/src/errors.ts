/**
 * Typed error classes for Zcash session management.
 *
 * Follows the pattern established by Tempo's error types in mppx.
 */

export class SessionNotFoundError extends Error {
  readonly code = 'SESSION_NOT_FOUND' as const
  constructor(sessionId: string) {
    super(`session not found: ${sessionId}`)
    this.name = 'SessionNotFoundError'
  }
}

export class SessionClosedError extends Error {
  readonly code = 'SESSION_CLOSED' as const
  constructor(sessionId: string, status: string) {
    super(`session is ${status}: ${sessionId}`)
    this.name = 'SessionClosedError'
  }
}

export class InsufficientBalanceError extends Error {
  readonly code = 'INSUFFICIENT_BALANCE' as const
  readonly needed: number
  readonly available: number
  constructor(needed: number, available: number) {
    super(`insufficient balance: need ${needed}, have ${available}`)
    this.name = 'InsufficientBalanceError'
    this.needed = needed
    this.available = available
  }
}

export class InvalidBearerError extends Error {
  readonly code = 'INVALID_BEARER' as const
  constructor() {
    super('invalid bearer')
    this.name = 'InvalidBearerError'
  }
}

export class DepositConsumedError extends Error {
  readonly code = 'DEPOSIT_CONSUMED' as const
  constructor(txid: string) {
    super(`deposit already consumed: ${txid}`)
    this.name = 'DepositConsumedError'
  }
}

export class DepositVerificationError extends Error {
  readonly code = 'DEPOSIT_VERIFICATION_FAILED' as const
  constructor(reason?: string) {
    super(reason ?? 'deposit verification failed')
    this.name = 'DepositVerificationError'
  }
}

export class TopUpConsumedError extends Error {
  readonly code = 'TOPUP_CONSUMED' as const
  constructor(txid: string) {
    super(`top-up already consumed: ${txid}`)
    this.name = 'TopUpConsumedError'
  }
}

export type SessionError =
  | SessionNotFoundError
  | SessionClosedError
  | InsufficientBalanceError
  | InvalidBearerError
  | DepositConsumedError
  | DepositVerificationError
  | TopUpConsumedError
