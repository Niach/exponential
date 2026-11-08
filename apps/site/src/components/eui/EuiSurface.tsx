/* genUI inside Astro: ONE React component, two ways to use it.
   - `<EuiSurface tree={…} />` (no client directive): Astro renders it at
     build time with react-dom/server; the page ships the HTML and the
     surface's CSS, zero JS.
   - `<EuiSurface client:visible tree={…} />`: the same HTML, hydrated when
     it scrolls into view (inputs, actions, Motion's WASM player).
   The site is dark only, so every surface paints dark. */
import { useMemo } from "react"
import { ExponentialSurface, useSurface } from "@exponential_app/eui-react"
import type { HostPlugin } from "@exponential_app/eui-react"

/** The host's icon map (name → component), as HostPlugin takes it. */
export type IconMap = NonNullable<HostPlugin[`icons`]>
import type { NestedNode } from "@exponential_app/eui"

export interface EuiSurfaceProps {
  id: string
  tree: NestedNode
  data?: Record<string, unknown>
  theme?: string
  /** Extra icons for the surface (build-time surfaces only: components do not serialize into an island). */
  icons?: IconMap
  /** Open overlays paint in place (build-time specimens: a portal renders nothing). */
  inlineOverlays?: boolean
  /** The page's origin (`Astro.url.origin`): a build-time surface has no
   *  document, so a site-relative picture (`/shots/…`) resolves against it. */
  origin?: string
}

const PRIVATE = /^(localhost|127\.0\.0\.1|\[::1\])$/

export default function EuiSurface({ id, tree, data, theme = `exponential`, icons, inlineOverlays, origin }: EuiSurfaceProps) {
  const surface = useSurface({ surfaceId: id, initial: tree, data })
  const { setData } = surface
  const host = useMemo<HostPlugin>(
    () => ({
      ...(icons ? { icons } : {}),
      ...(origin ? { media: { baseUrl: `${origin}/`, ...(PRIVATE.test(new URL(origin).hostname) ? { allowPrivate: [new URL(origin).hostname] } : {}) } } : {}),
      // A stub host: inputs echo into the data model, actions resolve.
      onInput: (e) => {
        if (e.path) setData(e.path, e.value)
        return Promise.resolve()
      },
      onAction: () => Promise.resolve(),
    }),
    [setData, icons, origin]
  )
  return <ExponentialSurface id={`xui-${id}`} surface={surface} theme={theme} mode="dark" host={host} inlineOverlays={inlineOverlays} />
}
