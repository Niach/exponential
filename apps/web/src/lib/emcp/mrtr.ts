// #NEW-144: MCP 2026-07-28 Multi Round-Trip Requests for `/api/mcp`
// (packages/emcp/fixtures/protocol.json `mrtr`, `stateCases`). A handler
// that needs the person (a confirmation, a choice) answers
// `askInput(ctx, {inputRequests, data})`: an `input_required` result whose
// `requestState` is AEAD-sealed (AES-256-GCM, key off BETTER_AUTH_SECRET)
// to the member, the credential, the method, the tool, a digest of the
// arguments and a TTL. The client retries the same call with
// `inputResponses` + the state. The server's `requestState.verify` hook
// (`requestStateVerifier`, set by server.ts for the ONE call a per-request
// server serves) opens it, checks the binding and spends it ONCE before any
// handler runs (#NEW-147: the spent id is a row of `mcp_request_states`, so
// a restart cannot replay it); a refusal answers JSON-RPC -32602 with the SDK's frozen
// wording (the reason goes to the server's onerror only). The handler then
// reads its `data` and the answers with `redeemInput(ctx)`.
import { createCipheriv, createDecipheriv, createHash, hkdfSync, randomBytes, randomUUID } from "node:crypto"
import { ProtocolError, type McpServer } from "@modelcontextprotocol/server"
import {
  argsCanonical,
  canDeliver,
  elicitationAnswer,
  inputRequiredResult,
  MRTR,
  PROTOCOL_ERRORS,
  redeemVerdict,
  type ElicitationAnswer,
  type InputRequest,
  type InputRequiredResult,
  type StateBinding,
  type StateClaims,
  type StateVerdict,
} from "@exp/emcp/protocol"
import type { EmcpContext, EmcpToolContext, Principal } from "./request-context"

export { elicitForm, elicitUrlRequest as elicitUrl, type InputRequest, type InputRequiredResult } from "@exp/emcp/protocol"

const IV_BYTES = 12
const TAG_BYTES = 16
const VERSION = 1

/** The -32602 every refused requestState answers (never the reason). */
export class RequestStateError extends ProtocolError {
  constructor(readonly reason: StateVerdict | `sealed` | `missing`) {
    super(PROTOCOL_ERRORS.invalidParams, PROTOCOL_ERRORS.invalidRequestState, { reason: `invalid_request_state` })
  }
}

let cachedKey: { secret: string; key: Buffer } | null = null

function stateKey(): Buffer {
  const secret = process.env.BETTER_AUTH_SECRET
  if (!secret) throw new Error(`BETTER_AUTH_SECRET is not set: MCP requestState cannot be sealed`)
  if (cachedKey?.secret !== secret) {
    cachedKey = { secret, key: Buffer.from(hkdfSync(`sha256`, secret, MRTR.stateKey.salt, MRTR.stateKey.info, 32)) }
  }
  return cachedKey.key
}

/** base64url SHA-256 of the canonical arguments. */
export function argsDigest(args: unknown): string {
  return createHash(`sha256`).update(argsCanonical(args), `utf8`).digest(`base64url`)
}

/** Seal claims into the wire string (`es1.` + base64url(iv | ct | tag)). */
export function sealState(claims: StateClaims): string {
  const iv = randomBytes(IV_BYTES)
  const cipher = createCipheriv(`aes-256-gcm`, stateKey(), iv)
  cipher.setAAD(Buffer.from(MRTR.stateKey.prefix))
  const body = Buffer.concat([cipher.update(JSON.stringify({ v: VERSION, ...claims }), `utf8`), cipher.final()])
  return `${MRTR.stateKey.prefix}${Buffer.concat([iv, body, cipher.getAuthTag()]).toString(`base64url`)}`
}

/** The claims of a sealed state, or null when it does not open. */
export function openState(wire: unknown): StateClaims | null {
  if (typeof wire !== `string` || !wire.startsWith(MRTR.stateKey.prefix)) return null
  try {
    const raw = Buffer.from(wire.slice(MRTR.stateKey.prefix.length), `base64url`)
    if (raw.length < IV_BYTES + TAG_BYTES + 2) return null
    const decipher = createDecipheriv(`aes-256-gcm`, stateKey(), raw.subarray(0, IV_BYTES))
    decipher.setAAD(Buffer.from(MRTR.stateKey.prefix))
    decipher.setAuthTag(raw.subarray(raw.length - TAG_BYTES))
    const text = Buffer.concat([decipher.update(raw.subarray(IV_BYTES, raw.length - TAG_BYTES)), decipher.final()]).toString(`utf8`)
    const claims = JSON.parse(text) as Partial<StateClaims> & { v?: unknown }
    const fields = [claims.sub, claims.grant, claims.method, claims.tool, claims.args, claims.jti]
    if (claims.v !== VERSION || fields.some((f) => typeof f !== `string`) || typeof claims.exp !== `number`) return null
    return claims as StateClaims
  } catch {
    return null
  }
}

/** Where redeemed ids live until they expire (single use). */
export interface RedeemStore {
  /** true the first time `jti` is claimed, false after. */
  claim(jti: string, expMs: number, now: number): boolean | Promise<boolean>
}

/** #NEW-147: THE store: one `mcp_request_states` row per spent id (a unique
 *  insert: the first claim wins across processes and restarts), swept by
 *  the watchdog once the state itself has expired. */
export function dbRedeemStore(): RedeemStore {
  return {
    async claim(jti, expMs) {
      const [{ db }, { mcpRequestStates }] = await Promise.all([import(`@/db/connection`), import(`@/db/schema`)])
      const rows = await db
        .insert(mcpRequestStates)
        .values({ jti: jti.slice(0, 64), expiresAt: new Date(expMs) })
        .onConflictDoNothing()
        .returning({ jti: mcpRequestStates.jti })
      return rows.length === 1
    },
  }
}

/** One process's memory (tests): a redeemed state is spent until its expiry. */
export function memoryRedeemStore(): RedeemStore {
  const spent = new Map<string, number>()
  return {
    claim(jti, expMs, now) {
      if (spent.size > 1024) for (const [id, exp] of spent) if (exp <= now) spent.delete(id)
      if (spent.has(jti)) return false
      spent.set(jti, expMs)
      return true
    },
  }
}

let redeemStore: RedeemStore = dbRedeemStore()

/** Tests swap the store. */
export function setRedeemStore(store: RedeemStore): void {
  redeemStore = store
}

export type McpCall = NonNullable<EmcpContext[`call`]>

/** The one call of a request that may carry a requestState: `tools/call`
 *  (the tool), `prompts/get` (the prompt), `resources/read` (the uri). */
export function callOf(message: unknown): McpCall | null {
  if (typeof message !== `object` || message === null || Array.isArray(message)) return null
  const { method, params } = message as { method?: unknown; params?: Record<string, unknown> }
  if (typeof method !== `string` || !MRTR.inputRequiredMethods.includes(method)) return null
  const tool = method === `resources/read` ? params?.uri : params?.name
  if (typeof tool !== `string`) return null
  const args = method === `resources/read` ? undefined : params?.arguments
  return { method, tool, args: (typeof args === `object` && args !== null ? args : {}) as Record<string, unknown> }
}

/** What a state of this call is bound to. */
export function bindingOf(principal: Principal, call: McpCall): StateBinding {
  return {
    sub: principal.userId,
    grant: principal.grant,
    method: call.method ?? `tools/call`,
    tool: call.tool,
    args: argsDigest(call.args),
  }
}

/** Mint a sealed state for this handler's call carrying `data`. */
export function mintState(ctx: EmcpToolContext, data?: unknown, now = Date.now()): string {
  if (!ctx.call) throw new Error(`MRTR needs the wire call on the context`)
  const call = { ...ctx.call, method: ctx.call.method ?? ctx.mcpReq?.method ?? `tools/call` }
  const claims: StateClaims = { ...bindingOf(ctx.principal, call), exp: now + MRTR.ttlSeconds * 1000, jti: randomUUID(), ...(data === undefined ? {} : { data }) }
  return sealState(claims)
}

/** Open, check and spend `wire` for this caller's call: the claims, or a
 *  RequestStateError naming why not. */
export async function verifyRequestState(wire: unknown, principal: Principal, call: McpCall | null | undefined, now = Date.now()): Promise<StateClaims> {
  const claims = openState(wire)
  if (!claims) throw new RequestStateError(`sealed`)
  if (!call) throw new RequestStateError(`missing`)
  const verdict = redeemVerdict(claims, bindingOf(principal, call), now, false)
  if (verdict !== `ok`) throw new RequestStateError(verdict)
  if (!(await redeemStore.claim(claims.jti, claims.exp, now))) throw new RequestStateError(`replayed`)
  return claims
}

/** The server's `requestState.verify` hook for the one call it serves
 *  (what it resolves is what `ctx.mcpReq.requestState()` returns). */
export function requestStateVerifier(context: Pick<EmcpContext, `principal` | `call`>): (state: string) => Promise<StateClaims> {
  return (state) => verifyRequestState(state, context.principal, context.call)
}

/**
 * The `input_required` answer asking the client for `inputRequests`, with a
 * sealed state carrying `data`. null when the client cannot be asked: an
 * era without MRTR (2025), or a request whose capability it did not declare
 * (the spec forbids sending it); the caller then goes on without asking.
 */
export function askInput(
  ctx: EmcpToolContext,
  spec: { inputRequests: Record<string, InputRequest>; data?: unknown },
  now = Date.now()
): InputRequiredResult | null {
  if (ctx.client.era !== `modern`) return null
  if (!MRTR.inputRequiredMethods.includes(ctx.mcpReq?.method ?? `tools/call`)) return null
  if (!Object.values(spec.inputRequests).every((request) => canDeliver(request, ctx.client))) return null
  return inputRequiredResult({ inputRequests: spec.inputRequests, requestState: mintState(ctx, spec.data, now) })
}

export interface Redeemed<T> {
  data: T | undefined
  /** The bare elicitation answer under `key`, null when missing. */
  answer(key: string): ElicitationAnswer | null
}

/**
 * The retry of an `askInput`: null when this call carries no state (a first
 * round); else the state the server's verify hook already opened, checked
 * and spent. A raw string means no verifier ran: refused, never trusted.
 */
export function redeemInput<T = unknown>(ctx: EmcpToolContext): Redeemed<T> | null {
  const state = ctx.mcpReq?.requestState?.()
  if (state === undefined || state === null) return null
  if (typeof state !== `object` || typeof (state as StateClaims).jti !== `string`) throw new RequestStateError(`sealed`)
  const responses = ctx.mcpReq?.inputResponses
  return { data: (state as StateClaims).data as T | undefined, answer: (key) => elicitationAnswer(responses, key) }
}

// #NEW-147: McpServer answers ANY throw of a tool handler as an `isError`
// tool result; the few JSON-RPC errors a call itself must answer (an
// elicitation answer that was not offered: -32602) escape through this
// slot, keyed by the request's abort signal, and its tools/call wrapper.
const rpcEscapes = new WeakMap<object, ProtocolError>()

/** Answer this tool call with the JSON-RPC `error` (outside an SDK request: thrown). */
export function escapeAsRpcError(ctx: Pick<EmcpToolContext, `mcpReq`>, error: ProtocolError): { content: { type: `text`; text: string }[]; isError: true } {
  const key = (ctx.mcpReq as { signal?: object } | undefined)?.signal
  if (!key) throw error
  rpcEscapes.set(key, error)
  return { content: [{ type: `text`, text: error.message }], isError: true }
}

/** Wraps `server`'s tools/call (registered by its first tool) to throw an escaped error. */
export function installRpcErrorEscape(server: McpServer): void {
  const handlers = (server.server as unknown as { _requestHandlers: Map<string, (request: unknown, ctx: { mcpReq?: { signal?: object } }) => Promise<unknown>> })._requestHandlers
  const inner = handlers.get(`tools/call`)
  if (!inner) return
  handlers.set(`tools/call`, async (request, ctx) => {
    const result = await inner(request, ctx)
    const key = ctx?.mcpReq?.signal
    const escaped = key ? rpcEscapes.get(key) : undefined
    if (escaped) throw escaped
    return result
  })
}

