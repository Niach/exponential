// eCore (#EXP-1504): the plain types every ExponentialOS module shares. The
// enums, constants and rules are generated from contract.json (the same file
// generates Rust, Swift and Kotlin); this file adds the TS shapes around them.

import {
  clampMessage,
  defaultRetryable,
  type Capability,
  type ErrorCode,
} from "./contract.generated"

export * from "./contract.generated"

/** What a caller holds: a token naming a session (and run), the capabilities it admits and when it ends (unix ms; null = until revoked). */
export interface Grant {
  token: string
  sessionId: string
  runId?: string | null
  capabilities: Capability[]
  expiresAt?: number | null
}

/** The one refusal/error shape. */
export interface Failure {
  code: ErrorCode
  message: string
  retryable: boolean
}

/** The message clamped, `retryable` from the code. */
export const failure = (code: ErrorCode, message: string): Failure => ({
  code,
  message: clampMessage(message),
  retryable: defaultRetryable(code),
})

/** The event envelope, the stable core of `wire`. */
export interface Envelope<T = unknown> {
  v: number
  id: string
  kind: string
  at: number
  source: string
  sessionId?: string | null
  data: T
}
