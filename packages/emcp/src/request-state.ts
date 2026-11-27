// #NEW-145 (MCP 2026-07-28 MRTR): the `requestState` Exponential's MCP
// server hands out with an `input_required` result (elicitation.json) or an
// `upload_required` one (files.json), and verifies on the retry. Locked by
// `fixtures/elicitation.json` `requestState` (its `$comment` is the spec).
// Server-only (node:crypto).

import { createHash, createHmac, timingSafeEqual } from "node:crypto"

import fixture from "../fixtures/elicitation.json" with { type: "json" }
import { canonicalJson } from "./pure"

const SPEC = fixture.requestState

export const REQUEST_STATE_PREFIX: string = SPEC.prefix
export const REQUEST_STATE_TTL_MS: number = SPEC.ttlMs

export type RequestStateKind = `elicit` | `upload`

/** What a state binds: who, which call, why, until when, and its one use. */
export interface RequestStatePayload {
  /** The caller's principal (`user:<id>|client:<clientId>`). */
  principal: string
  /** The tool name of the call. */
  tool: string
  /** `argsDigest(arguments)` of the call. */
  args: string
  kind: RequestStateKind
  /** The keys this state waits on: input request keys (elicit) or staged file ids (upload). */
  keys: string[]
  /** Unix ms after which the state is refused. */
  exp: number
  /** A random id: a state is consumed once. */
  nonce: string
}

export type RequestStateError = `malformed` | `tampered` | `expired` | `foreign` | `mismatch` | `replayed`

const b64url = (bytes: Buffer) => bytes.toString(`base64url`)

/** sha256 hex of the canonical JSON of a call's `arguments` (none = `{}`). */
export function argsDigest(args: unknown): string {
  return createHash(`sha256`).update(canonicalJson(args ?? {}), `utf8`).digest(`hex`)
}

function wire(payload: RequestStatePayload): string {
  return canonicalJson({ a: payload.args, e: payload.exp, k: payload.kind, n: payload.nonce, p: payload.principal, s: payload.keys, t: payload.tool })
}

function mac(key: string, signed: string): Buffer {
  return createHmac(`sha256`, key).update(signed, `utf8`).digest()
}

/** `rs1.<payload>.<mac>`: base64url canonical JSON + base64url HMAC-SHA256 over `rs1.<payload>`. */
export function sealRequestState(key: string, payload: RequestStatePayload): string {
  const body = b64url(Buffer.from(wire(payload), `utf8`))
  const signed = `${REQUEST_STATE_PREFIX}.${body}`
  return `${signed}.${b64url(mac(key, signed))}`
}

export interface VerifyContext {
  key: string
  principal: string
  tool: string
  args: unknown
  now: number
  /** Nonces already used (the server's consumed set, kept until their exp). */
  consumed: ReadonlySet<string>
}

export type VerifyResult = { ok: true; payload: RequestStatePayload } | { ok: false; error: RequestStateError }

/** Check a retried call's `requestState`, in the order the fixture names. */
export function verifyRequestState(token: unknown, ctx: VerifyContext): VerifyResult {
  if (typeof token !== `string` || token.length > SPEC.maxLength) return { ok: false, error: `malformed` }
  const parts = token.split(`.`)
  if (parts.length !== 3 || parts[0] !== REQUEST_STATE_PREFIX || !parts[1] || !parts[2]) return { ok: false, error: `malformed` }
  const expected = mac(ctx.key, `${parts[0]}.${parts[1]}`)
  const given = Buffer.from(parts[2], `base64url`)
  if (given.length !== expected.length || !timingSafeEqual(given, expected)) return { ok: false, error: `tampered` }
  let raw: Record<string, unknown>
  try {
    raw = JSON.parse(Buffer.from(parts[1], `base64url`).toString(`utf8`)) as Record<string, unknown>
  } catch {
    return { ok: false, error: `malformed` }
  }
  const payload: RequestStatePayload = {
    principal: String(raw.p),
    tool: String(raw.t),
    args: String(raw.a),
    kind: raw.k as RequestStateKind,
    keys: Array.isArray(raw.s) ? (raw.s as string[]) : [],
    exp: Number(raw.e),
    nonce: String(raw.n),
  }
  if (!(ctx.now < payload.exp)) return { ok: false, error: `expired` }
  if (payload.principal !== ctx.principal) return { ok: false, error: `foreign` }
  if (payload.tool !== ctx.tool || payload.args !== argsDigest(ctx.args)) return { ok: false, error: `mismatch` }
  if (ctx.consumed.has(payload.nonce)) return { ok: false, error: `replayed` }
  return { ok: true, payload }
}

/** The JSON-RPC error a refused state answers (always -32602). */
export function requestStateError(error: RequestStateError): { code: number; message: string } {
  return { code: SPEC.errorCode, message: SPEC.errors[error] }
}
