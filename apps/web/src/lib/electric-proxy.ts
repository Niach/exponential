import "@dotenvx/dotenvx/config"
import { promisify } from "node:util"
import { gzip } from "node:zlib"

/**
 * REV-29: the ONLY inbound query params the proxy forwards to Electric —
 * opaque protocol/cursor state, nothing that can carry SQL or widen a shape.
 * Deliberately NOT the dependency's `ELECTRIC_PROTOCOL_QUERY_PARAMS`
 * (@electric-sql/client): that array is third-party-owned and now includes
 * `subset__where`/`subset__params`/…, a client-supplied SQL predicate Electric
 * evaluates over ALL columns regardless of the server-pinned `columns`
 * allowlist — forwarding it would let any member filter on the very columns
 * the allowlists hide (invite tokens, subscriber emails). New upstream params
 * stay dropped here until someone adds them on purpose.
 */
const FORWARDED_ELECTRIC_PARAMS = new Set([
  `live`,
  `live_sse`,
  `experimental_live_sse`,
  `handle`,
  `offset`,
  `cursor`,
  `expired_handle`,
  `log`,
  `cache-buster`,
])

/**
 * REV-8: compression must be ASYNC. A snapshot chunk can run to multiple MB
 * and zlib does ~50MB/s on JSON, so the synchronous variant froze the single
 * Bun.serve JS thread for ~hundreds of ms per chunk — and a cold-start herd
 * (deploy restart, shape-identity rotation) serialized seconds of dead event
 * loop through it, stalling every concurrent long-poll, tRPC mutation and SSR
 * request. The callback form runs on the threadpool instead.
 */
const gzipAsync = promisify(gzip)

/**
 * Returns the Electric SQL endpoint URL: the `ELECTRIC_URL` env var if set,
 * otherwise the local docker endpoint on the default port 30000.
 */
function getElectricUrl(): string {
  return process.env.ELECTRIC_URL || `http://localhost:30000`
}

/**
 * Prepares the Electric SQL proxy URL from a request URL
 * Copies over Electric-specific query params and adds auth if configured
 * @param requestUrl - The incoming request URL
 * @returns The prepared Electric SQL origin URL
 */
export function prepareElectricUrl(requestUrl: string): URL {
  const url = new URL(requestUrl)
  const electricUrl = getElectricUrl()
  const originUrl = new URL(`${electricUrl}/v1/shape`)

  // Copy Electric-specific query params
  url.searchParams.forEach((value, key) => {
    if (FORWARDED_ELECTRIC_PARAMS.has(key)) {
      originUrl.searchParams.set(key, value)
    }
  })

  // Add Electric Cloud authentication if configured
  if (process.env.ELECTRIC_SOURCE_ID && process.env.ELECTRIC_SECRET) {
    originUrl.searchParams.set(`source_id`, process.env.ELECTRIC_SOURCE_ID)
    originUrl.searchParams.set(`secret`, process.env.ELECTRIC_SECRET)
  }

  return originUrl
}

/**
 * REV2-5: bound on concurrently-proxied SNAPSHOT-CLASS requests — every
 * request WITHOUT `live=true`. That is the initial snapshot (`offset=-1`) AND
 * all of its continuation chunks (REV-27): Electric splits a snapshot bigger
 * than its chunk threshold (~10MB) into chunks the client follows with plain
 * non-live GETs at `offset=<electric-offset>`, so for a large shape the bulk
 * of the data arrives at offsets != -1. Every one of those bodies is buffered
 * wholly in Bun memory below, and they herd: every client cold start (or
 * shape-identity rotation) fires one pipeline per shape, with the client
 * prefetching up to 2 chunks ahead. Excess snapshot-class requests queue FIFO
 * instead of buffering concurrently, so a herd degrades to added latency, not
 * unbounded heap. Live long-polls (`live=true`, incl. the SSE variants) are
 * never gated: their bodies are tiny and they'd hold slots for the whole poll
 * window.
 *
 * Sized at 32 ≈ one full client's shape count (18 since EXP-481) plus a few
 * large shapes' chunk-prefetch pipelines and headroom (EXP-264): at 8, a
 * SINGLE cold-starting client queued its own second half behind its first, so
 * the shapes that arrived last were the ones the app opened onto —
 * stale-looking state on launch. It is still a herd bound: a multi-client
 * storm queues, it just never makes one client wait on itself. The worst-case
 * heap is bounded by slots × Electric's chunk threshold, not by shape size.
 */
const SNAPSHOT_PROXY_CONCURRENCY = 32

let activeSnapshotProxies = 0
const snapshotWaiters: Array<() => void> = []

function releaseSnapshotSlot(): void {
  const next = snapshotWaiters.shift()
  // Hand the slot to the next waiter (the active count transfers); only
  // decrement when nobody is queued.
  if (next) next()
  else activeSnapshotProxies--
}

/**
 * Resolves true once a snapshot slot is held, false if the caller aborted
 * while queued (the caller must NOT release in that case — it never held a
 * slot).
 */
function acquireSnapshotSlot(signal?: AbortSignal): Promise<boolean> {
  if (signal?.aborted) return Promise.resolve(false)
  if (activeSnapshotProxies < SNAPSHOT_PROXY_CONCURRENCY) {
    activeSnapshotProxies++
    return Promise.resolve(true)
  }
  return new Promise<boolean>((resolve) => {
    const waiter = () => {
      signal?.removeEventListener(`abort`, onAbort)
      resolve(true)
    }
    const onAbort = () => {
      const index = snapshotWaiters.indexOf(waiter)
      if (index !== -1) snapshotWaiters.splice(index, 1)
      resolve(false)
    }
    snapshotWaiters.push(waiter)
    signal?.addEventListener(`abort`, onAbort, { once: true })
  })
}

/**
 * Proxies a request to Electric SQL and returns the response.
 *
 * Buffers the upstream body fully before responding so the Bun server can
 * send a properly-framed HTTP/1.1 response with a known content-length —
 * streaming `response.body` directly produced chunked-encoding tails that
 * Traefik logged as `EOF` → 502. Because of that buffering, snapshot-class
 * (non-live) requests pass through the semaphore above. Forwarding the inbound
 * AbortSignal cancels the upstream when the browser hangs up (very common:
 * the Electric client cancels long-polls every time a shape handle is
 * invalidated).
 */
/**
 * A request is live iff it carries an affirmative `live` param (the SSE
 * variants ride alongside it, but count on their own for safety — gating a
 * held-open stream would pin a slot for its whole lifetime). Everything else
 * is snapshot-class: the initial `offset=-1` request AND the non-live
 * continuation chunks the client follows it with (REV-27). The check is on
 * the value, not mere presence, so `live=false` — which Electric answers with
 * snapshot data — cannot skip the gate.
 */
function isLiveRequest(originUrl: URL): boolean {
  return (
    originUrl.searchParams.get(`live`) === `true` ||
    originUrl.searchParams.get(`live_sse`) === `true` ||
    originUrl.searchParams.get(`experimental_live_sse`) === `true`
  )
}

export async function proxyElectricRequest(
  originUrl: URL,
  signal?: AbortSignal,
  acceptEncoding?: string | null
): Promise<Response> {
  const isSnapshot = !isLiveRequest(originUrl)
  if (isSnapshot) {
    const acquired = await acquireSnapshotSlot(signal)
    if (!acquired) {
      // Client hung up while queued — nothing to send back.
      return new Response(null, {
        status: 499,
        statusText: `Client Closed Request`,
      })
    }
  }
  try {
    return await proxyElectricRequestInner(originUrl, signal, acceptEncoding)
  } finally {
    if (isSnapshot) releaseSnapshotSlot()
  }
}

/**
 * Below this, compression costs more than it saves: an idle live long-poll's
 * `[{"headers":{"control":"up-to-date"}}]` is ~40 bytes and gzip would make it
 * bigger. Snapshots — the bodies worth compressing — are orders of magnitude
 * past this.
 */
const GZIP_MIN_BYTES = 1024

/** Does this client's `Accept-Encoding` allow a gzip response? */
function acceptsGzip(acceptEncoding: string | null | undefined): boolean {
  if (!acceptEncoding) return false
  return acceptEncoding
    .split(`,`)
    .some((part) => part.trim().toLowerCase().split(`;`)[0] === `gzip`)
}

async function proxyElectricRequestInner(
  originUrl: URL,
  signal?: AbortSignal,
  acceptEncoding?: string | null
): Promise<Response> {
  let response: Response
  try {
    response = await fetch(originUrl, { signal })
  } catch (error) {
    if (signal?.aborted || (error instanceof Error && error.name === `AbortError`)) {
      // Client hung up before upstream responded — nothing to send back.
      return new Response(null, { status: 499, statusText: `Client Closed Request` })
    }
    return new Response(`Upstream fetch failed`, { status: 502 })
  }

  let body: ArrayBuffer
  try {
    body = await response.arrayBuffer()
  } catch (error) {
    if (signal?.aborted || (error instanceof Error && error.name === `AbortError`)) {
      return new Response(null, { status: 499, statusText: `Client Closed Request` })
    }
    return new Response(`Upstream body read failed`, { status: 502 })
  }

  const headers = new Headers(response.headers)
  headers.delete(`content-encoding`)
  headers.delete(`content-length`)
  headers.delete(`transfer-encoding`)
  // Electric snapshot responses ship `cache-control: public, max-age=604800`
  // with no auth-aware vary. Shape data is per-user (the where clause is
  // derived from the caller's credentials), so any HTTP cache that stores it
  // can serve one user's snapshot to another — macOS URLCache did exactly
  // that, replaying an anonymous snapshot to an authed client. Force
  // never-cache on every proxied shape response; keep vary as a second
  // line of defense for caches that ignore no-store. It must list every
  // credential the shape route accepts: cookie, authorization, AND x-api-key.
  headers.set(`cache-control`, `private, no-store`)

  // Compress on the way out (EXP-304). Bun's `fetch` already decoded whatever
  // Electric sent, and the body is fully buffered here anyway, so gzipping it
  // costs one pass over memory we are holding regardless. Electric's JSON
  // compresses roughly 10x, which is the difference between a snapshot being a
  // moment and being a wait on a phone. Strictly opt-in per request:
  // URLSession and OkHttp advertise gzip and decode transparently, ureq
  // (desktop) only when built with its gzip feature, and anything that doesn't
  // ask still gets plain JSON.
  //
  // `vary` must therefore now include accept-encoding as well as every
  // credential the shape route accepts — a cache that ignores `no-store` must
  // not hand a gzipped body to a client that never asked for one.
  headers.set(`vary`, `authorization, cookie, x-api-key, accept-encoding`)

  if (
    acceptsGzip(acceptEncoding) &&
    body.byteLength >= GZIP_MIN_BYTES &&
    // Never double-encode: if upstream really did hand back an encoded body
    // (Bun decodes automatically, so this is belt-and-braces), leave it alone.
    !response.headers.get(`content-encoding`)
  ) {
    const compressed = await gzipAsync(new Uint8Array(body))
    headers.set(`content-encoding`, `gzip`)
    headers.set(`content-length`, String(compressed.byteLength))
    return new Response(compressed, {
      status: response.status,
      statusText: response.statusText,
      headers,
    })
  }

  return new Response(body, {
    status: response.status,
    statusText: response.statusText,
    headers,
  })
}
