// FEED-131: Radix Avatar's image-load state machine.

import { createComputed, createSignal, onCleanup } from "solid-js"

export type LoadingStatus = `idle` | `loading` | `loaded` | `error`

/** Radix Avatar's `useImageLoadingStatus`: a detached `Image` probes the
 *  src; the <img> paints only once it LOADED. */
export function createImageLoadingStatus(src: () => string | undefined): () => LoadingStatus {
  const [status, setStatus] = createSignal<LoadingStatus>(`idle`)
  const image = typeof window === `undefined` ? null : new window.Image()
  const resolve = (s: string | undefined): LoadingStatus => {
    if (!image) return `idle`
    if (!s) return `error`
    if (image.src !== s) image.src = s
    return image.complete && image.naturalWidth > 0 ? `loaded` : `loading`
  }
  createComputed(() => setStatus(resolve(src())))
  if (image) {
    const loaded = () => setStatus(`loaded`)
    const errored = () => setStatus(`error`)
    image.addEventListener(`load`, loaded)
    image.addEventListener(`error`, errored)
    onCleanup(() => {
      image.removeEventListener(`load`, loaded)
      image.removeEventListener(`error`, errored)
    })
  }
  return status
}
