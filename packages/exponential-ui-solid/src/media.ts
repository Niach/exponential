// FEED-128: the media loader on Solid (React's media.ts). The policy, the
// bounded blob-url cache and the limited fetchers are the web layer's
// (`openMediaSource`); this follows a source: a fetched one loads once per
// mounted consumer set (released on cleanup), a direct one is used as is, a
// denied one is a failed load.

import { createEffect, createMemo, createSignal, onCleanup, type Accessor } from "solid-js"
import { openMediaSource, type MediaKind } from "@exponential_app/eui/web"
import type { HostPlugin } from "./host"

export { clearMediaCache, fetchLimited, fetchStream, mediaCacheStats, MEDIA_CACHE_IDLE_MAX } from "@exponential_app/eui/web"
export type { MediaKind } from "@exponential_app/eui/web"

export interface MediaSourceState {
  url: string | undefined
  error: boolean
}

/** A media source's state, live: the url to use (undefined while a fetched
 *  one loads or after it failed) and whether the fetch failed. A src the
 *  media policy denies has no url and `error`. */
export function createMediaSource(host: Accessor<HostPlugin>, src: Accessor<string>, kind: MediaKind = `image`): Accessor<MediaSourceState> {
  const plan = createMemo(() => openMediaSource(host(), src(), kind))
  const key = createMemo(() => (plan().fetched ? plan().key : ``))
  const [blob, setBlob] = createSignal<{ for: string; url?: string; error?: true } | null>(null)
  createEffect(() => {
    const k = key()
    if (!k) return
    let live = true
    const { promise, release } = plan().open()
    promise.then(
      (url) => live && setBlob({ for: k, url }),
      () => live && setBlob({ for: k, error: true })
    )
    onCleanup(() => {
      live = false
      release()
    })
  })
  return createMemo(() => {
    const p = plan()
    if (!p.fetched) return { url: p.direct, error: p.denied }
    const b = blob()
    return b?.for === p.key ? { url: b.url, error: b.error === true } : { url: undefined, error: false }
  }, undefined, { equals: (a, b) => a.url === b.url && a.error === b.error })
}

/** The src an element should use, or undefined while a fetched one loads
 *  (or after it failed). */
export function createMediaSrc(host: Accessor<HostPlugin>, src: Accessor<string>, kind: MediaKind = `image`): Accessor<string | undefined> {
  const source = createMediaSource(host, src, kind)
  return () => source().url
}
