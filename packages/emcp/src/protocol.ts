// #NEW-144: the protocol base of Exponential's MCP endpoints for MCP
// 2026-07-28 beside the 2025 handshake: the era of a request, the client's
// per-request capabilities (`ctx.client`), the MRTR builders and the
// requestState redemption verdict. Pure (no node: modules) and locked by
// `fixtures/protocol.json` (its `$comment` is the spec); the sealing itself
// (AES-256-GCM) is the server's (`apps/web/src/lib/emcp/mrtr.ts`).

import fixture from "../fixtures/protocol.json" with { type: "json" }
import { canonicalJson } from "./pure"

export const MODERN_VERSIONS: readonly string[] = fixture.versions.modern
export const LEGACY_VERSIONS: readonly string[] = fixture.versions.legacy
export const META_KEYS = fixture.metaKeys
export const EXTENSION_IDS = fixture.extensionIds
export const MRTR = fixture.mrtr
export const PROTOCOL_ERRORS = fixture.errors
/** The extensions Exponential's own server declares today. */
export const SERVER_EXTENSIONS = fixture.serverExtensions as Readonly<Record<string, Record<string, unknown>>>

export type Era = `modern` | `legacy`

/** What the caller of ONE request declared (handlers read it as `ctx.client`). */
export interface ClientContext {
  era: Era
  protocolVersion: string | null
  info: { name: string; version: string } | null
  capabilities: Record<string, unknown>
  extensions: Record<string, unknown>
  elicitation: { form: boolean; url: boolean }
  sampling: boolean
  roots: boolean
  tasks: boolean
  ui: boolean
}

const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === `object` && value !== null && !Array.isArray(value)

function infoOf(value: unknown): ClientContext[`info`] {
  if (!isObject(value) || typeof value.name !== `string` || typeof value.version !== `string`) return null
  return { name: value.name, version: value.version }
}

function contextOf(era: Era, protocolVersion: string | null, info: unknown, capabilities: unknown): ClientContext {
  const caps = isObject(capabilities) ? capabilities : {}
  const extensions = isObject(caps.extensions) ? caps.extensions : {}
  const elicitation = isObject(caps.elicitation) ? caps.elicitation : null
  // `elicitation: {}` is form mode only (the spec's backward-compatible form).
  const bare = elicitation !== null && !(`form` in elicitation) && !(`url` in elicitation)
  return {
    era,
    protocolVersion,
    info: infoOf(info),
    capabilities: caps,
    extensions,
    elicitation: {
      form: elicitation !== null && (bare || isObject(elicitation.form)),
      url: elicitation !== null && isObject(elicitation.url),
    },
    sampling: isObject(caps.sampling),
    roots: isObject(caps.roots),
    tasks: isObject(extensions[EXTENSION_IDS.tasks]),
    ui: isObject(extensions[EXTENSION_IDS.ui]),
  }
}

/** The client of one JSON-RPC message: the 2026-07-28 envelope when its
 *  `params._meta` carries one, an `initialize`'s own capabilities, else a
 *  legacy client that declared nothing. */
export function clientContextOf(message: unknown): ClientContext {
  if (!isObject(message)) return contextOf(`legacy`, null, null, null)
  const params = isObject(message.params) ? message.params : {}
  const meta = isObject(params._meta) ? params._meta : {}
  const version = meta[META_KEYS.protocolVersion]
  if (typeof version === `string`) {
    return contextOf(`modern`, version, meta[META_KEYS.clientInfo], meta[META_KEYS.clientCapabilities])
  }
  if (message.method === `initialize`) {
    const requested = typeof params.protocolVersion === `string` ? params.protocolVersion : null
    return contextOf(`legacy`, requested, params.clientInfo, params.capabilities)
  }
  return contextOf(`legacy`, null, null, null)
}

// ---------------------------------------------------------------- MRTR

export interface InputRequest {
  method: string
  params?: Record<string, unknown>
}

export interface InputRequiredResult {
  resultType: `input_required`
  inputRequests?: Record<string, InputRequest>
  requestState?: string
}

/** `input_required` with at least one of inputRequests / requestState. */
export function inputRequiredResult(spec: { inputRequests?: Record<string, InputRequest>; requestState?: string }): InputRequiredResult {
  const requests = spec.inputRequests && Object.keys(spec.inputRequests).length > 0 ? spec.inputRequests : undefined
  if (!requests && spec.requestState === undefined) throw new TypeError(`input_required needs inputRequests or requestState`)
  return {
    resultType: `input_required`,
    ...(requests ? { inputRequests: requests } : {}),
    ...(spec.requestState !== undefined ? { requestState: spec.requestState } : {}),
  }
}

/** A form-mode `elicitation/create` input request. */
export function elicitForm(message: string, requestedSchema: Record<string, unknown>): InputRequest {
  return { method: `elicitation/create`, params: { mode: `form`, message, requestedSchema } }
}

/** A url-mode `elicitation/create` input request (2026-07-28: no elicitationId). */
export function elicitUrl(message: string, url: string): InputRequest {
  return { method: `elicitation/create`, params: { mode: `url`, url, message } }
}

/** May this input request go to a client with these capabilities? */
export function canDeliver(request: InputRequest, client: Pick<ClientContext, `elicitation` | `sampling` | `roots`>): boolean {
  switch (request.method) {
    case `elicitation/create`:
      return (request.params?.mode ?? `form`) === `url` ? client.elicitation.url : client.elicitation.form
    case `sampling/createMessage`:
      return client.sampling
    case `roots/list`:
      return client.roots
    default:
      return false
  }
}

export interface ElicitationAnswer {
  action: `accept` | `decline` | `cancel`
  /** The accepted form content; null on decline / cancel or a non-object. */
  content: Record<string, unknown> | null
}

/** The bare elicitation answer under `key` of a retry's inputResponses. */
export function elicitationAnswer(responses: unknown, key: string): ElicitationAnswer | null {
  if (!isObject(responses)) return null
  const entry = responses[key]
  if (!isObject(entry)) return null
  const action = entry.action
  if (action !== `accept` && action !== `decline` && action !== `cancel`) return null
  return { action, content: action === `accept` && isObject(entry.content) ? entry.content : null }
}

// ---------------------------------------------------------------- requestState

/** The text the args digest hashes: audit.json's canonical JSON of the
 *  call's arguments (none = `{}`), the same text as the audit's args_hash. */
export function argsCanonical(args: unknown): string {
  return canonicalJson(args ?? {})
}

/** What a sealed requestState is bound to. */
export interface StateBinding {
  /** The member. */
  sub: string
  /** The credential: `oauth:<clientId>`, `key:<keyId>` or `member`. */
  grant: string
  method: string
  tool: string
  /** base64url SHA-256 of `argsCanonical(arguments)`. */
  args: string
}

export interface StateClaims extends StateBinding {
  /** Expiry, in the clock's unit (ms on the server). */
  exp: number
  /** The single-use id. */
  jti: string
  data?: unknown
}

export type StateVerdict = `ok` | `expired` | `member` | `grant` | `method` | `tool` | `args` | `replayed`

/** The first check a redemption fails, in the fixture's order, or `ok`. */
export function redeemVerdict(claims: StateClaims, expected: StateBinding, now: number, used: boolean): StateVerdict {
  if (now >= claims.exp) return `expired`
  if (claims.sub !== expected.sub) return `member`
  if (claims.grant !== expected.grant) return `grant`
  if (claims.method !== expected.method) return `method`
  if (claims.tool !== expected.tool) return `tool`
  if (claims.args !== expected.args) return `args`
  if (used) return `replayed`
  return `ok`
}
