// Round 1: the platform preferences a surface follows LIVE — dark mode for
// `mode: system`, `prefers-contrast: more` for `contrast: system`, reduced
// motion, a hover-capable pointer — and the surface's own measured box
// (breakpoints and height/orientation conditions resolve against the
// SURFACE, never the window).

import { useCallback, useEffect, useLayoutEffect, useState, useSyncExternalStore } from "react"

const useIsoLayoutEffect = typeof window === `undefined` ? useEffect : useLayoutEffect

/** `matchMedia(query).matches`, live; `fallback` without `matchMedia`
 *  (SSR, jsdom). */
export function useMediaQuery(query: string, fallback = false): boolean {
  const subscribe = useCallback(
    (onChange: () => void) => {
      if (typeof window === `undefined` || typeof window.matchMedia !== `function`) return () => {}
      const mql = window.matchMedia(query)
      mql.addEventListener?.(`change`, onChange)
      return () => mql.removeEventListener?.(`change`, onChange)
    },
    [query]
  )
  const read = () => (typeof window === `undefined` || typeof window.matchMedia !== `function` ? fallback : window.matchMedia(query).matches)
  return useSyncExternalStore(subscribe, read, () => fallback)
}

export interface BoxSize {
  width: number
  height: number
  /** False until the first measurement. */
  measured: boolean
}

/** An element's border box, live (ResizeObserver, disconnected on unmount). */
export function useElementSize(el: HTMLElement | null, initialWidth = 0): BoxSize {
  const [size, setSize] = useState<BoxSize>({ width: initialWidth, height: 0, measured: false })
  useIsoLayoutEffect(() => {
    if (!el) return
    const read = () => {
      const r = el.getBoundingClientRect()
      setSize((cur) => {
        // No layout yet (SSR, jsdom, display:none): keep the initial guess.
        if (!cur.measured && r.width === 0 && r.height === 0) return cur
        return cur.measured && cur.width === r.width && cur.height === r.height ? cur : { width: r.width, height: r.height, measured: true }
      })
    }
    read()
    if (typeof ResizeObserver === `undefined`) return
    const ro = new ResizeObserver(read)
    ro.observe(el)
    return () => ro.disconnect()
  }, [el])
  return size
}
