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

/** The src an element should use, or undefined while a fetched one loads. */
export function useMediaSrc(host: HostPlugin, src: string): string | undefined {
  const req = src && host.mediaRequest ? host.mediaRequest(src) : null
  const headers = req?.headers ?? {}
  const fetched = req !== null && Object.keys(headers).length > 0
  const direct = !src ? undefined : req ? req.url : host.resolveUrl ? host.resolveUrl(src) : src
  const [blob, setBlob] = useState<{ for: string; url: string } | null>(null)
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
    p.then((url) => live && setBlob({ for: k, url })).catch(() => {})
    return () => {
      live = false
    }
    // req/headers are derived from k
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fetched, k])
  if (!fetched) return direct
  return blob?.for === k ? blob.url : undefined
}
