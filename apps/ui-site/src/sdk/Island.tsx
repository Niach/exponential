/* Client-only islands. The prerender (renderToString under Bun) and the
   first client render both paint a sized placeholder; the SDK renderer and
   the icon set load in an effect after hydration, so every page prerenders
   and hydrates without a mismatch, and no page chunk carries the renderer. */
import { useEffect, useState, type CSSProperties, type ReactNode, type RefObject } from "react"
import type { IconMap } from "@exponential-at/ui-react"
import type { LiveSurfaceProps } from "./runtime"

export interface SdkRuntime {
  LiveSurface: (props: LiveSurfaceProps) => ReactNode
  icons: IconMap
}

let runtime: Promise<SdkRuntime> | null = null

/** The renderer + the full icon map, loaded once per page. */
export function loadRuntime(): Promise<SdkRuntime> {
  runtime ??= Promise.all([import("./runtime"), import("./icons")]).then(([r, i]) => ({ LiveSurface: r.LiveSurface, icons: i.siteIcons }))
  return runtime
}

/** The runtime once loaded (null during SSR and the first client render). */
export function useRuntime(enabled = true): SdkRuntime | null {
  const [rt, setRt] = useState<SdkRuntime | null>(null)
  useEffect(() => {
    if (!enabled) return
    let live = true
    loadRuntime().then((r) => live && setRt(r))
    return () => {
      live = false
    }
  }, [enabled])
  return rt
}

/** The placeholder a surface stands in for until the renderer is there. */
export function SurfacePlaceholder({ minHeight = 160, label = `Loading the renderer…`, style }: { minHeight?: number | string; label?: string; style?: CSSProperties }) {
  return (
    <div className="sdk-placeholder" style={{ minHeight, ...style }} aria-busy="true">
      <span>{label}</span>
    </div>
  )
}

/** True after hydration (false in the prerender and the first client render). */
export function useHydrated(): boolean {
  const [hydrated, setHydrated] = useState(false)
  useEffect(() => setHydrated(true), [])
  return hydrated
}

/* Many small live renders on one page (the index, a variants gallery) mount
   only once near the viewport, one per idle slice, so the main thread never
   takes them all in one long task. */
const queue: (() => void)[] = []
let draining = false
const idle = (cb: () => void) => (typeof requestIdleCallback === `function` ? requestIdleCallback(cb, { timeout: 120 }) : setTimeout(cb, 16))

function drain() {
  draining = true
  idle(() => {
    const started = performance.now()
    while (queue.length && performance.now() - started < 12) queue.shift()!()
    if (queue.length) drain()
    else draining = false
  })
}

/** True once the element came within `margin` of the viewport and its turn
 *  in the mount queue came (stays true). */
export function useNearViewport(ref: RefObject<Element | null>, margin = `300px`): boolean {
  const [ready, setReady] = useState(false)
  useEffect(() => {
    const el = ref.current
    if (!el || ready) return
    let cancelled = false
    const io = new IntersectionObserver(
      (entries) => {
        if (!entries.some((e) => e.isIntersecting)) return
        io.disconnect()
        queue.push(() => !cancelled && setReady(true))
        if (!draining) drain()
      },
      { rootMargin: margin }
    )
    io.observe(el)
    return () => {
      cancelled = true
      io.disconnect()
    }
  }, [ref, margin, ready])
  return ready
}
