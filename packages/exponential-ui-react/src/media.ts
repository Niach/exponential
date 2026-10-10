// VAPP-91: the media loader. A source resolves through the host's
// `mediaRequest` (absolute url + headers, the policy hook that adds auth for
// /api/attachments) after `resolveUrl`, always under the media policy
// (VAPP-103: schemes + hosts, `urls.ts`; a denied src loads nothing); a
// request with headers is fetched once — under `MEDIA_LIMITS` (bytes,
// timeout) — and shown as a blob url (an <img> cannot send headers). A
// direct <img> load is the browser's (its own decode limits).
//
// Video/Audio are STREAMS (`kind: "stream"`, catalog/host.json: the policy
// applies, the byte limits do not): a request with headers goes to the
// host's `mediaStreamUrl` (a signed / cookie-authed url the player streams
// and seeks, re-checked against the policy) when it has one, else it is
// fetched whole into a blob url with no byte, pixel or total-time cap
// (only the wait for the response headers is timed). A poster is an image.
//
// Blob urls pin their bytes until revoked, so the cache is bounded: every
// mounted consumer holds a reference; an entry nobody holds stays cached
// (a re-mounted row, a virtualized list scrolling back, shows it at once)
// until more than MEDIA_CACHE_IDLE_MAX idle entries exist, when the least
// recently released one is dropped and its url revoked. An entry in use is
// never evicted.

import { useEffect, useState } from "react"
import { MEDIA_LIMITS, imageDimensions } from "@exponential-at/ui"
import type { HostPlugin } from "./host"
import { mediaRequestOf, mediaUrlAllowed } from "./urls"

/** What a source feeds: an image (the limits hold) or a player stream. */
export type MediaKind = `image` | `stream`

/** How many fetched media nobody shows stay cached (their blob urls live). */
export const MEDIA_CACHE_IDLE_MAX = 64

interface Entry {
  promise: Promise<string>
  /** Set once the fetch resolved. */
  url?: string
  /** Mounted consumers. */
  refs: number
}

/** Insertion order = recency (acquire/release move an entry to the end). */
const blobs = new Map<string, Entry>()

function key(url: string, headers: Record<string, string>): string {
  return `${url}\n${JSON.stringify(headers)}`
}

function touch(k: string, e: Entry): void {
  blobs.delete(k)
  blobs.set(k, e)
}

function acquire(k: string, load: () => Promise<string>): Entry {
  let e = blobs.get(k)
  if (!e) {
    const entry: Entry = { refs: 0, promise: Promise.resolve(``) }
    entry.promise = load().then((url) => {
      entry.url = url
      // Evicted (or failed over) while loading: nobody can use it.
      if (blobs.get(k) !== entry) URL.revokeObjectURL(url)
      return url
    })
    entry.promise.catch(() => {
      if (blobs.get(k) === entry) blobs.delete(k)
    })
    e = entry
  }
  e.refs += 1
  touch(k, e)
  return e
}

function release(k: string, e: Entry): void {
  e.refs -= 1
  if (e.refs > 0 || blobs.get(k) !== e) return
  touch(k, e)
  let idle = 0
  for (const v of blobs.values()) if (v.refs === 0) idle += 1
  for (const [ok, v] of blobs) {
    if (idle <= MEDIA_CACHE_IDLE_MAX) break
    if (v.refs > 0) continue
    blobs.delete(ok)
    if (v.url) URL.revokeObjectURL(v.url)
    idle -= 1
  }
}

/** The cache's size (tests). */
export function mediaCacheStats(): { entries: number; inUse: number } {
  let inUse = 0
  for (const v of blobs.values()) if (v.refs > 0) inUse += 1
  return { entries: blobs.size, inUse }
}

/** Drop every idle entry and revoke its url (tests, a host that logs out). */
export function clearMediaCache(): void {
  for (const [k, v] of blobs) {
    if (v.refs > 0) continue
    blobs.delete(k)
    if (v.url) URL.revokeObjectURL(v.url)
  }
}

/** A media source's state: the url to use (undefined while a fetched one
 *  loads or after it failed) and whether the fetch failed (so an Image can
 *  show its fallback; an <img> never sees a failed fetch). A src the media
 *  policy denies has no url and `error`. */
export function useMediaSource(host: HostPlugin, src: string, kind: MediaKind = `image`): { url: string | undefined; error: boolean } {
  const req = mediaRequestOf(host, src)
  const headers = req?.headers ?? {}
  const fetched = req !== null && Object.keys(headers).length > 0
  const direct = req ? req.url : undefined
  const [blob, setBlob] = useState<{ for: string; url?: string; error?: true } | null>(null)
  const k = fetched ? `${kind}\n${key(req.url, headers)}` : ``
  useEffect(() => {
    if (!fetched) return
    let live = true
    const hosted = kind === `stream` && host.mediaStreamUrl !== undefined
    const entry = hosted ? streamEntry(host, req) : acquire(k, () => (kind === `stream` ? fetchStream(req.url, headers) : fetchLimited(req.url, headers)).then((b) => URL.createObjectURL(b)))
    entry.promise.then(
      (url) => live && setBlob({ for: k, url }),
      () => live && setBlob({ for: k, error: true })
    )
    return () => {
      live = false
      if (!hosted) release(k, entry)
    }
    // req/headers are derived from k
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fetched, k])
  // A src the media policy denies is a failed load (the fallback paints).
  if (!fetched) return { url: direct, error: Boolean(src) && req === null }
  return blob?.for === k ? { url: blob.url, error: blob.error === true } : { url: undefined, error: false }
}

/** A host stream url (never cached here, never revoked: the host owns
 *  it), re-checked against the media policy. */
function streamEntry(host: HostPlugin, req: { url: string; headers: Record<string, string> }): Entry {
  const promise = Promise.resolve(host.mediaStreamUrl!(req)).then((url) => {
    if (typeof url !== `string` || !mediaUrlAllowed(host, url)) throw new Error(`stream url denied by the media policy`)
    return url
  })
  return { promise, refs: 0 }
}

/** A Video/Audio fetch (no `mediaStreamUrl`): the whole body into a blob,
 *  no byte or pixel cap; only the wait for the response headers is timed
 *  (`MEDIA_LIMITS.timeoutMs`), the download itself may take as long as it
 *  takes. */
export async function fetchStream(url: string, headers: Record<string, string>, timeoutMs: number = MEDIA_LIMITS.timeoutMs): Promise<Blob> {
  const abort = new AbortController()
  const timer = setTimeout(() => abort.abort(new Error(`media timed out after ${timeoutMs} ms`)), timeoutMs)
  let r: Response
  try {
    r = await fetch(url, { headers, signal: abort.signal })
  } finally {
    clearTimeout(timer)
  }
  if (!r.ok) throw new Error(`HTTP ${r.status}`)
  return r.blob()
}

/** A fetch under `MEDIA_LIMITS`: the whole request within `timeoutMs`, the
 *  body (Content-Length up front, then as it streams) within `maxBytes`, an
 *  image header's width × height within `maxPixels` (before any decode). */
export async function fetchLimited(url: string, headers: Record<string, string>, limits: { maxBytes: number; timeoutMs: number; maxPixels: number } = MEDIA_LIMITS): Promise<Blob> {
  const abort = new AbortController()
  const timer = setTimeout(() => abort.abort(new Error(`media timed out after ${limits.timeoutMs} ms`)), limits.timeoutMs)
  const over = () => new Error(`media is over ${limits.maxBytes} bytes`)
  try {
    const r = await fetch(url, { headers, signal: abort.signal })
    if (!r.ok) throw new Error(`HTTP ${r.status}`)
    if (Number(r.headers.get(`content-length`) ?? NaN) > limits.maxBytes) throw over()
    const chunks: Uint8Array[] = []
    let total = 0
    if (r.body) {
      const reader = r.body.getReader()
      for (;;) {
        const { done, value } = await reader.read()
        if (done) break
        total += value.byteLength
        if (total > limits.maxBytes) {
          abort.abort()
          throw over()
        }
        chunks.push(value)
      }
    } else {
      const all = new Uint8Array(await r.arrayBuffer())
      if (all.byteLength > limits.maxBytes) throw over()
      chunks.push(all)
    }
    const head = chunks.length === 1 ? chunks[0]! : concat(chunks, 64 * 1024)
    const dims = imageDimensions(head)
    if (dims && dims[0] * dims[1] > limits.maxPixels) throw new Error(`media is ${dims[0]}×${dims[1]}, over ${limits.maxPixels} pixels`)
    return new Blob(chunks as BlobPart[], { type: r.headers.get(`content-type`) ?? `` })
  } finally {
    clearTimeout(timer)
  }
}

function concat(chunks: readonly Uint8Array[], max: number): Uint8Array {
  const out = new Uint8Array(Math.min(max, chunks.reduce((n, c) => n + c.byteLength, 0)))
  let at = 0
  for (const c of chunks) {
    if (at >= out.length) break
    const part = c.subarray(0, out.length - at)
    out.set(part, at)
    at += part.length
  }
  return out
}

/** The src an element should use, or undefined while a fetched one loads
 *  (or after it failed). */
export function useMediaSrc(host: HostPlugin, src: string, kind: MediaKind = `image`): string | undefined {
  return useMediaSource(host, src, kind).url
}
