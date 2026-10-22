// FEED-128: the platform preferences a surface follows LIVE (dark mode for
// `mode: system`, `prefers-contrast: more`, reduced motion, a hover-capable
// pointer) and an element's measured box, as Solid primitives (React's
// platform.ts). Listeners are removed when the owner is disposed.

import { createEffect, createSignal, getOwner, onCleanup, type Accessor } from "solid-js"

/** `matchMedia(query).matches`, live; `fallback` without `matchMedia`
 *  (SSR, jsdom). */
export function createMediaQuery(query: string, fallback = false): Accessor<boolean> {
  if (typeof window === `undefined` || typeof window.matchMedia !== `function`) return () => fallback
  const mql = window.matchMedia(query)
  const [matches, setMatches] = createSignal(mql.matches)
  const onChange = () => setMatches(mql.matches)
  mql.addEventListener?.(`change`, onChange)
  if (getOwner()) onCleanup(() => mql.removeEventListener?.(`change`, onChange))
  return matches
}

export interface BoxSize {
  width: number
  height: number
  /** False until the first measurement. */
  measured: boolean
}

/** An element's border box, live (ResizeObserver, disconnected on cleanup). */
export function createElementSize(el: Accessor<HTMLElement | null | undefined>, initialWidth = 0): Accessor<BoxSize> {
  const [size, setSize] = createSignal<BoxSize>({ width: initialWidth, height: 0, measured: false })
  createEffect(() => {
    const node = el()
    if (!node) return
    const read = () => {
      const r = node.getBoundingClientRect()
      const cur = size()
      // No layout yet (SSR, jsdom, display:none): keep the initial guess.
      if (!cur.measured && r.width === 0 && r.height === 0) return
      if (cur.measured && cur.width === r.width && cur.height === r.height) return
      setSize({ width: r.width, height: r.height, measured: true })
    }
    read()
    if (typeof ResizeObserver === `undefined`) return
    const ro = new ResizeObserver(read)
    ro.observe(node)
    onCleanup(() => ro.disconnect())
  })
  return size
}
