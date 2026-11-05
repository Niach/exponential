/* genUI inside Astro: ONE React component, two ways to use it.
   - `<EuiSurface tree={…} />` (no client directive): Astro renders it at
     build time with react-dom/server; the page ships the HTML and the
     surface's CSS, zero JS.
   - `<EuiSurface client:visible tree={…} />`: the same HTML, hydrated when
     it scrolls into view (inputs, actions, Motion's WASM player).
   The surface follows the site's theme switch (Starlight's data-theme). */
import { useEffect, useMemo, useState } from "react"
import { ExponentialSurface, useSurface } from "@exponential_app/eui-react"
import type { HostPlugin } from "@exponential_app/eui-react"

/** The host's icon map (name → component), as HostPlugin takes it. */
export type IconMap = NonNullable<HostPlugin[`icons`]>
import type { NestedNode } from "@exponential_app/eui"

type Mode = `light` | `dark`

function useSiteMode(): Mode {
  // The build has no theme; dark is the site default (Starlight's too).
  const [mode, setMode] = useState<Mode>(`dark`)
  useEffect(() => {
    const read = () => setMode(document.documentElement.dataset.theme === `light` ? `light` : `dark`)
    read()
    const mo = new MutationObserver(read)
    mo.observe(document.documentElement, { attributes: true, attributeFilter: [`data-theme`] })
    return () => mo.disconnect()
  }, [])
  return mode
}

export interface EuiSurfaceProps {
  id: string
  tree: NestedNode
  data?: Record<string, unknown>
  theme?: string
  /** Extra icons for the surface (build-time surfaces only: components do not serialize into an island). */
  icons?: IconMap
  /** Open overlays paint in place (build-time specimens: a portal renders nothing). */
  inlineOverlays?: boolean
}

export default function EuiSurface({ id, tree, data, theme = `exponential`, icons, inlineOverlays }: EuiSurfaceProps) {
  const surface = useSurface({ surfaceId: id, initial: tree, data })
  const mode = useSiteMode()
  const { setData } = surface
  const host = useMemo<HostPlugin>(
    () => ({
      ...(icons ? { icons } : {}),
      // A stub host: inputs echo into the data model, actions resolve.
      onInput: (e) => {
        if (e.path) setData(e.path, e.value)
        return Promise.resolve()
      },
      onAction: () => Promise.resolve(),
    }),
    [setData, icons]
  )
  useEffect(() => {
    document.dispatchEvent(new Event(`exp:surface-mounted`))
  }, [])
  return <ExponentialSurface id={`xui-${id}`} surface={surface} theme={theme} mode={mode} host={host} inlineOverlays={inlineOverlays} />
}
