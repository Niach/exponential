// #LOGS-2: `POST /api/monitoring`, the Sentry tunnel every client sends its
// envelopes through (the SDKs' `tunnel` option). It reads ONLY the envelope
// header (the first line: `{"dsn": …}`) or the request's `sentry_key` /
// `?platform=` (`resolveProject`), accepts only the projects of the DSNs
// configured in this instance (config.ts `allowedDsns`; else 403) and forwards
// with the server's own key for that project, rate-limits per signed-in user,
// else per client IP, caps the body at ~1 MB and forwards the untouched bytes
// to GLITCHTIP_INTERNAL_URL with a FRESH header set: no cookie, no
// authorization, no X-Forwarded-For / X-Real-IP / Forwarded, so GlitchTip
// never learns the client's address. The payload is never logged.

import { gunzipSync, inflateSync } from "node:zlib"
import {
  allowedDsns,
  dsnFor,
  glitchtipInternalUrl,
  internalEnvelopeUrl,
  isMonitoringPlatform,
  parseDsn,
  type ParsedDsn,
} from "./config"

/** The cap on the body, as received AND once decompressed (a zip bomb stops here). */
export const MAX_ENVELOPE_BYTES = 1024 * 1024

type Env = Record<string, string | undefined>

export type TunnelDeps = {
  env: Env
  fetch: (url: string, init: RequestInit) => Promise<Response>
  /** The signed-in user's id, or null (anonymous). Only called after the cheap checks. */
  resolveUserId: (request: Request) => Promise<string | null>
  /** The limiter key's client address (proxy-attested, see lib/widget/rate-limit.ts). */
  clientIp: (request: Request) => string
  takeToken: (key: string) => { ok: true } | { ok: false; retryAfterSeconds: number }
}

/**
 * The headers forwarded upstream: built from scratch, never copied, so no
 * client address (X-Forwarded-For, X-Real-IP, Forwarded, CF-Connecting-IP …),
 * cookie or credential can leave with the envelope. Only the content
 * encoding (gzip/deflate, the bytes are forwarded as received) and an
 * X-Sentry-Auth carrying the configured key of the resolved project.
 */
export function upstreamHeaders(request: Request, target?: ParsedDsn): Headers {
  const headers = new Headers({
    "content-type": `application/x-sentry-envelope`,
  })
  // The resolved project's OWN key, never the client's.
  if (target) {
    headers.set(
      `x-sentry-auth`,
      `Sentry sentry_version=7, sentry_client=exponential-tunnel/1, sentry_key=${target.publicKey}`
    )
  }
  const encoding = request.headers.get(`content-encoding`)?.trim().toLowerCase()
  if (encoding === `gzip` || encoding === `deflate`) {
    headers.set(`content-encoding`, encoding)
  }
  return headers
}

/**
 * The envelope as plain bytes: decompressed for `gzip` / `deflate` (never
 * past MAX_ENVELOPE_BYTES: `too-large`), as-is for none/`identity`;
 * `unreadable` for any other encoding or a corrupt stream.
 */
export function decodeEnvelope(
  body: Uint8Array,
  contentEncoding: string | null
): Uint8Array | `too-large` | `unreadable` {
  const encoding = contentEncoding?.trim().toLowerCase()
  if (!encoding || encoding === `identity`) return body
  if (encoding !== `gzip` && encoding !== `deflate`) return `unreadable`
  try {
    const inflate = encoding === `gzip` ? gunzipSync : inflateSync
    return inflate(body, { maxOutputLength: MAX_ENVELOPE_BYTES })
  } catch (error) {
    return (error as { code?: string }).code === `ERR_BUFFER_TOO_LARGE`
      ? `too-large`
      : `unreadable`
  }
}

/** The envelope header's DSN, or null (not an envelope, unreadable, no dsn). */
export function envelopeDsn(body: Uint8Array, contentEncoding: string | null): string | null {
  const bytes = decodeEnvelope(body, contentEncoding)
  if (typeof bytes === `string`) return null
  const newline = bytes.indexOf(0x0a)
  const first = new TextDecoder().decode(newline === -1 ? bytes : bytes.subarray(0, newline))
  try {
    const header = JSON.parse(first) as { dsn?: unknown }
    return typeof header.dsn === `string` ? header.dsn : null
  } catch {
    return null
  }
}

/** The body, or null past `limit` bytes (stops reading at the cap). */
async function readCapped(request: Request, limit: number): Promise<Uint8Array | null> {
  const declared = Number(request.headers.get(`content-length`))
  if (Number.isFinite(declared) && declared > limit) return null
  if (!request.body) return new Uint8Array()
  const reader = request.body.getReader()
  const chunks: Uint8Array[] = []
  let total = 0
  for (;;) {
    const { done, value } = await reader.read()
    if (done) break
    total += value.byteLength
    if (total > limit) {
      void reader.cancel().catch(() => {})
      return null
    }
    chunks.push(value)
  }
  const out = new Uint8Array(total)
  let offset = 0
  for (const chunk of chunks) {
    out.set(chunk, offset)
    offset += chunk.byteLength
  }
  return out
}

/** The `sentry_key` of an `X-Sentry-Auth` header (`Sentry sentry_key=…, sentry_version=7`). */
export function sentryAuthKey(header: string | null): string | null {
  const match = header ? /(?:^|[\s,])sentry_key=([^,\s]+)/.exec(header) : null
  return match ? match[1] : null
}

/**
 * The configured project an envelope is for, or null (= 403). In order:
 * (1) the envelope header's `dsn` (its project id; host and scheme ignored),
 * (2) the `sentry_key` of `X-Sentry-Auth` or `?sentry_key=` matched against
 * the configured DSNs' public keys (sentry-android sends no `dsn` header),
 * (3) `?platform=` mapped to that platform's configured DSN. Always the
 * SERVER's configured DSN comes back: its key is what is forwarded, never a
 * client-supplied host or key.
 */
export function resolveProject(
  request: Request,
  body: Uint8Array,
  env: Env,
  allowed: Map<string, ParsedDsn>
): ParsedDsn | null {
  const fromHeader = parseDsn(envelopeDsn(body, request.headers.get(`content-encoding`)) ?? undefined)
  if (fromHeader) return allowed.get(fromHeader.projectId) ?? null

  const url = new URL(request.url)
  const key =
    sentryAuthKey(request.headers.get(`x-sentry-auth`)) ?? url.searchParams.get(`sentry_key`)
  if (key) {
    for (const dsn of allowed.values()) if (dsn.publicKey === key) return dsn
    return null
  }

  const platform = url.searchParams.get(`platform`)
  if (isMonitoringPlatform(platform)) {
    const dsn = parseDsn(dsnFor(env, platform) ?? undefined)
    if (dsn && allowed.has(dsn.projectId)) return allowed.get(dsn.projectId) ?? null
  }
  return null
}

const plain = (status: number, extra?: HeadersInit) =>
  new Response(null, { status, headers: extra })

export async function handleMonitoringTunnel(
  request: Request,
  deps: TunnelDeps
): Promise<Response> {
  const internal = glitchtipInternalUrl(deps.env)
  const allowed = allowedDsns(deps.env)
  if (!internal || allowed.size === 0) return plain(404)

  const body = await readCapped(request, MAX_ENVELOPE_BYTES)
  if (!body) return plain(413)
  if (decodeEnvelope(body, request.headers.get(`content-encoding`)) === `too-large`) {
    return plain(413)
  }

  const target = resolveProject(request, body, deps.env, allowed)
  if (!target) return plain(403)

  const userId = await deps.resolveUserId(request).catch(() => null)
  const verdict = deps.takeToken(userId ? `u:${userId}` : `ip:${deps.clientIp(request)}`)
  if (!verdict.ok) {
    return plain(429, { "retry-after": String(verdict.retryAfterSeconds) })
  }

  try {
    const upstream = await deps.fetch(internalEnvelopeUrl(internal, target), {
      method: `POST`,
      headers: upstreamHeaders(request, target),
      body: body as BodyInit,
    })
    // Pass the status (and a rate-limit hint) back so the SDKs back off;
    // never the upstream body.
    const headers = new Headers()
    for (const name of [`retry-after`, `x-sentry-rate-limits`]) {
      const value = upstream.headers.get(name)
      if (value) headers.set(name, value)
    }
    void upstream.body?.cancel().catch(() => {})
    return plain(upstream.status, headers)
  } catch {
    return plain(502)
  }
}
