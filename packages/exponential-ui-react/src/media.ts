// VAPP-91: the media loader. A source resolves through the host's
// `mediaRequest` (absolute url + headers, the policy hook that adds auth for
// /api/attachments) or `resolveUrl`; a request with headers is fetched once
// and shown as a blob url (an <img> cannot send headers).

import { useEffect, useState } from "react"
import type { HostPlugin } from "./host"

const blobs = new Map<string, Promise<string>>()

function key(url: string, headers: Record<string, string>): string {
  return `${url}\n${JSON.stringify(headers)}`
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
    let p = blobs.get(k)
    if (!p) {
      p = fetch(req.url, { headers })
        .then((r) => (r.ok ? r.blob() : Promise.reject(new Error(`HTTP ${r.status}`))))
        .then((b) => URL.createObjectURL(b))
      p.catch(() => blobs.delete(k))
      blobs.set(k, p)
    }
    p.then(
      (url) => live && setBlob({ for: k, url }),
      () => live && setBlob({ for: k, error: true })
    )
    return () => {
      live = false
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
