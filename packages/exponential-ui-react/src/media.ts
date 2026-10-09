// VAPP-91: the media loader. A source resolves through the host's
// `mediaRequest` (absolute url + headers, the policy hook that adds auth for
// /api/attachments) or `resolveUrl`; a request with headers is fetched once
// and shown as a blob url (an <img> cannot send headers).
//
// Blob urls pin their bytes until revoked, so the cache is bounded: every
// mounted consumer holds a reference; an entry nobody holds stays cached
// (a re-mounted row, a virtualized list scrolling back, shows it at once)
// until more than MEDIA_CACHE_IDLE_MAX idle entries exist, when the least
// recently released one is dropped and its url revoked. An entry in use is
// never evicted.

import { useEffect, useState } from "react"
import type { HostPlugin } from "./host"

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
 *  show its fallback; an <img> never sees a failed fetch). */
export function useMediaSource(host: HostPlugin, src: string): { url: string | undefined; error: boolean } {
  const req = src && host.mediaRequest ? host.mediaRequest(src) : null
  const headers = req?.headers ?? {}
  const fetched = req !== null && Object.keys(headers).length > 0
  const direct = !src ? undefined : req ? req.url : host.resolveUrl ? host.resolveUrl(src) : src
  const [blob, setBlob] = useState<{ for: string; url?: string; error?: true } | null>(null)
  const k = fetched ? key(req.url, headers) : ``
  useEffect(() => {
    if (!fetched) return
    let live = true
    const entry = acquire(k, () =>
      fetch(req.url, { headers })
        .then((r) => (r.ok ? r.blob() : Promise.reject(new Error(`HTTP ${r.status}`))))
        .then((b) => URL.createObjectURL(b))
    )
    entry.promise.then(
      (url) => live && setBlob({ for: k, url }),
      () => live && setBlob({ for: k, error: true })
    )
    return () => {
      live = false
      release(k, entry)
    }
    // req/headers are derived from k
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fetched, k])
  if (!fetched) return { url: direct, error: false }
  return blob?.for === k ? { url: blob.url, error: blob.error === true } : { url: undefined, error: false }
}

/** The src an element should use, or undefined while a fetched one loads
 *  (or after it failed). */
export function useMediaSrc(host: HostPlugin, src: string): string | undefined {
  return useMediaSource(host, src).url
}
