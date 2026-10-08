/* Client-only islands. The prerender (renderToString under Bun) and the
   first client render both paint a sized placeholder; the SDK renderer and
   the icon set load in an effect after hydration, so every page prerenders
   and hydrates without a mismatch, and no page chunk carries the renderer. */
import { useEffect, useState, type CSSProperties, type ReactNode } from "react"
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
