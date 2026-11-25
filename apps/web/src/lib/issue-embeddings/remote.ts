// #EXP-2133: the client of the host's one embedder process
// (`embedder/server.ts`). Never throws: an unreachable, restarting (OOM-killed),
// busy or older embedder answers `null`, exactly like a broken local worker.
//
// PRIVACY: issue text goes to `ISSUE_EMBEDDER_URL` and nowhere else, so that
// URL must be a machine-local one: a container name or a private address. A
// public host is refused (semantic search off, lexical search unaffected), a
// redirect is an error, and nothing is retried elsewhere.
import { isIP } from "node:net"
import embeddingModel from "../../../embedder/model.json"
import type { EmbedKind } from "../../../embedder/local"

/** Longer than the embedder's own load (~1 s) plus a queue; search stops waiting long before. */
const QUERY_TIMEOUT_MS = 10 * 1000
/** A passage waits behind every queued query and passage of all web containers. */
const PASSAGE_TIMEOUT_MS = 2 * 60 * 1000
/** After an unreachable embedder, nothing is tried for this long. */
const UNREACHABLE_COOLDOWN_MS = 30 * 1000
/** Passages in flight beyond this are dropped (the sweep finds them again). */
const MAX_INFLIGHT_PASSAGES = 32
/** More than the model reads (512 tokens); the embedder refuses a larger body. */
export const EMBEDDER_MAX_TEXT_CHARS = 64_000
export const EMBEDDER_MIN_SECRET_CHARS = 32

export type RemoteEmbedder = { ok: true; url: string; secret: string } | { ok: false; reason: string }

type Env = Record<string, string | undefined>

/** A container name, `localhost`, `*.internal`, or a loopback/private/link-local address. */
export function isHostLocal(hostname: string): boolean {
  const host = hostname.replace(/^\[|\]$/g, ``).toLowerCase()
  if (isIP(host) === 4) {
    const [a, b] = host.split(`.`).map(Number) as [number, number]
    return a === 127 || a === 10 || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168)
  }
  if (isIP(host) === 6) return host === `::1` || /^f[cd]/.test(host)
  return /^[a-z0-9]([a-z0-9_-]*[a-z0-9])?$/.test(host) || /^[a-z0-9_.-]+\.internal$/.test(host)
}

/** The configured embedder service, or null when `ISSUE_EMBEDDER_URL` is unset (= in-process). */
export function remoteEmbedderFrom(env: Env): RemoteEmbedder | null {
  const raw = env.ISSUE_EMBEDDER_URL?.trim()
  if (!raw) return null
  const secret = env.ISSUE_EMBEDDER_SECRET?.trim() ?? ``
  let url: URL
  try {
    url = new URL(raw)
  } catch {
    return { ok: false, reason: `ISSUE_EMBEDDER_URL is not a URL` }
  }
  if (url.protocol !== `http:` && url.protocol !== `https:`) return { ok: false, reason: `ISSUE_EMBEDDER_URL must be http(s)` }
  if (!isHostLocal(url.hostname)) {
    return { ok: false, reason: `ISSUE_EMBEDDER_URL must be a container name or a private address: issue text never leaves the host` }
  }
  if (secret.length < EMBEDDER_MIN_SECRET_CHARS) {
    return { ok: false, reason: `ISSUE_EMBEDDER_SECRET must be at least ${EMBEDDER_MIN_SECRET_CHARS} characters` }
  }
  return { ok: true, url: `${url.origin}/embed`, secret }
}

let cached: { key: string; value: RemoteEmbedder | null } | null = null

/** `remoteEmbedderFrom(process.env)`, a misconfiguration logged once. */
export function remoteEmbedder(): RemoteEmbedder | null {
  const key = `${process.env.ISSUE_EMBEDDER_URL ?? ``}\n${process.env.ISSUE_EMBEDDER_SECRET ?? ``}`
  if (cached?.key === key) return cached.value
  const value = remoteEmbedderFrom(process.env)
  cached = { key, value }
  if (value && !value.ok) console.error(`[issue-embeddings] ${value.reason}: semantic search is off`)
  return value
}

const stats = { embeds: 0, totalMs: 0, failures: 0, loads: 0, lastLoadMs: 0 }
let inflight = 0
let inflightPassages = 0
let downUntil = 0

export function remoteEmbedderStats(): Readonly<typeof stats> & { loaded: boolean; queued: number } {
  return { ...stats, loaded: false, queued: inflight }
}

/** Forgets the cooldown, the counters and the parsed configuration (tests). */
export function resetRemoteEmbedder(): void {
  cached = null
  downUntil = 0
  Object.assign(stats, { embeds: 0, totalMs: 0, failures: 0 })
}

function failed(reason: string, cooldown: boolean): null {
  stats.failures += 1
  if (cooldown) {
    if (Date.now() >= downUntil) console.error(`[issue-embeddings] embedder service: ${reason}`)
    downUntil = Date.now() + UNREACHABLE_COOLDOWN_MS
  }
  return null
}

export async function remoteEmbedText(
  remote: Extract<RemoteEmbedder, { ok: true }>,
  kind: EmbedKind,
  text: string,
  fetchImpl: typeof fetch = fetch
): Promise<Float32Array | null> {
  if (Date.now() < downUntil) return null
  if (kind === `passage`) {
    if (inflightPassages >= MAX_INFLIGHT_PASSAGES) return null
    inflightPassages += 1
  }
  inflight += 1
  const startMs = performance.now()
  try {
    const res = await fetchImpl(remote.url, {
      method: `POST`,
      headers: { "content-type": `application/json`, authorization: `Bearer ${remote.secret}` },
      body: JSON.stringify({ kind, text: text.slice(0, EMBEDDER_MAX_TEXT_CHARS), model: embeddingModel.id }),
      redirect: `error`,
      signal: AbortSignal.timeout(kind === `query` ? QUERY_TIMEOUT_MS : PASSAGE_TIMEOUT_MS),
    })
    if (!res.ok) {
      await res.body?.cancel().catch(() => {})
      // 503 = the queue is full or its worker just broke; 401/409 (wrong
      // secret, another model) do not heal by themselves: back off alike.
      return failed(`HTTP ${res.status}`, true)
    }
    const vector = new Float32Array(await res.arrayBuffer())
    if (vector.length !== embeddingModel.dimensions) return failed(`${vector.length} dimensions`, true)
    stats.embeds += 1
    stats.totalMs += performance.now() - startMs
    return vector
  } catch (err) {
    // A query that timed out is a slow embedder, not a dead one.
    const timedOut = (err as Error)?.name === `TimeoutError`
    return failed(String((err as Error)?.message ?? err), !timedOut || kind === `passage`)
  } finally {
    inflight -= 1
    if (kind === `passage`) inflightPassages -= 1
  }
}
