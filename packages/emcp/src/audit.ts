// EXP-1490: the `mcp_calls` audit row, one per MCP call. Pure and
// byte-locked by `fixtures/audit.json` (its `$comment` is the spec).

import { createHash } from "node:crypto"

import fixture from "../fixtures/audit.json" with { type: "json" }

export const CALL_STATUSES = fixture.statuses as readonly CallStatus[]
export const AUDIT_RETENTION_DAYS: number = fixture.retentionDays

export type CallStatus = `ok` | `denied` | `invalid` | `failed` | `upstream`

export { canonicalJson } from "./pure"
import { canonicalJson } from "./pure"

/** The row's `args_hash`: `sha256:` + the hex SHA-256 of the canonical
 *  JSON of the call's arguments (none = `{}`). Never the arguments. */
export function argsHash(args: unknown): string {
  const text = canonicalJson(args ?? {})
  return `${fixture.hashPrefix}${createHash(`sha256`).update(text, `utf8`).digest(`hex`)}`
}

export interface CallOutcome {
  /** The tool was not listed for the caller, or a call-time check refused. */
  denied?: boolean
  /** The arguments failed the tool's input schema. */
  invalid?: boolean
  /** The upstream answered an error or could not be reached. */
  upstreamError?: boolean
  /** The run itself failed (the result is an error). */
  error?: boolean
}

/** The row's `status`, first match wins: denied, invalid, upstream,
 *  failed, ok. */
export function callStatus(outcome: CallOutcome): CallStatus {
  if (outcome.denied) return `denied`
  if (outcome.invalid) return `invalid`
  if (outcome.upstreamError) return `upstream`
  if (outcome.error) return `failed`
  return `ok`
}

/** Is a row created at `createdAt` past retention at `now` (ISO times)? */
export function auditExpired(createdAt: string, now: string): boolean {
  return Date.parse(now) - Date.parse(createdAt) >= fixture.retentionDays * 86_400_000
}
