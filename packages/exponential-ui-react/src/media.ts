// VAPP-91: the media loader on React. The policy, the bounded blob-url
// cache and the limited fetchers are the web layer's
// (`@exponential-at/ui/web` media.ts, FEED-128); this is the hook around
// `openMediaSource`: a fetched source loads once per mounted consumer set
// (the cache holds it while any consumer is mounted), a direct one is used
// as is, a denied one is a failed load.

import { useEffect, useState } from "react"
import { openMediaSource, type MediaKind } from "@exponential-at/ui/web"
import type { HostPlugin } from "./host"

export { clearMediaCache, fetchLimited, fetchStream, mediaCacheStats, MEDIA_CACHE_IDLE_MAX } from "@exponential-at/ui/web"
export type { MediaKind } from "@exponential-at/ui/web"

/** A media source's state: the url to use (undefined while a fetched one
 *  loads or after it failed) and whether the fetch failed (so an Image can
 *  show its fallback; an <img> never sees a failed fetch). A src the media
 *  policy denies has no url and `error`. */
export function useMediaSource(host: HostPlugin, src: string, kind: MediaKind = `image`): { url: string | undefined; error: boolean } {
  const plan = openMediaSource(host, src, kind)
  const [blob, setBlob] = useState<{ for: string; url?: string; error?: true } | null>(null)
  const k = plan.key
  useEffect(() => {
    if (!plan.fetched) return
    let live = true
    const { promise, release } = plan.open()
    promise.then(
      (url) => live && setBlob({ for: k, url }),
      () => live && setBlob({ for: k, error: true })
    )
    return () => {
      live = false
      release()
    }
    // the plan is derived from k
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [plan.fetched, k])
  // A src the media policy denies is a failed load (the fallback paints).
  if (!plan.fetched) return { url: plan.direct, error: plan.denied }
  return blob?.for === k ? { url: blob.url, error: blob.error === true } : { url: undefined, error: false }
}

/** The src an element should use, or undefined while a fetched one loads
 *  (or after it failed). */
export function useMediaSrc(host: HostPlugin, src: string, kind: MediaKind = `image`): string | undefined {
  return useMediaSource(host, src, kind).url
}
