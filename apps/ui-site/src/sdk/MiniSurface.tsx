/* A small live render (the index cards, the variants gallery): mounts near
   the viewport, scaled by `zoom` into a fixed box, so it never shifts the
   layout. The prerender paints the empty box in the page scheme's ground;
   `mode` (the live scheme) reaches only the renderer. */
import { useEffect, useMemo, useRef, useState, type RefObject } from "react"
import { flatten, surfaceMessages, type Nested } from "./a2ui"
import { groundVars } from "./backgrounds"
import { useNearViewport, useRuntime } from "./Island"

export function MiniSurface({
  domId,
  tree,
  theme = `exponential`,
  mode,
  ground,
  zoom = 1,
  minWidth,
  inert,
  className,
}: {
  domId: string
  tree: Nested
  theme?: string
  mode: `light` | `dark`
  /** The theme's grounds; the page scheme picks one in CSS. */
  ground?: { light: string; dark: string }
  zoom?: number
  /** The narrowest logical width the specimen lays out at: a narrow box
   *  (two cards a row on a phone) zooms out further instead of squeezing
   *  the layout until words break. */
  minWidth?: number
  inert?: boolean
  className?: string
}) {
  const ref = useRef<HTMLDivElement>(null)
  const near = useNearViewport(ref)
  const runtime = useRuntime(near)
  const messages = useMemo(() => surfaceMessages(`mini`, flatten(tree)), [tree])
  const box = useContentWidth(ref, minWidth !== undefined)
  const scale = minWidth && box > 0 ? Math.min(zoom, box / minWidth) : zoom
  return (
    <div ref={ref} className={`sdk-mini${className ? ` ${className}` : ``}`} style={groundVars(ground)} inert={inert || undefined}>
      {runtime ? (
        <div className="sdk-mini-inner" style={scale === 1 ? undefined : { zoom: scale, width: `${100 / scale}%` }}>
          <runtime.LiveSurface surfaceId="mini" domId={domId} messages={messages} theme={theme} mode={mode} icons={runtime.icons} />
        </div>
      ) : (
        <span className="sdk-mini-wait" aria-hidden="true" />
      )}
    </div>
  )
}

/** The box's content width, live (0 until measured or when off). */
function useContentWidth(ref: RefObject<HTMLElement | null>, on: boolean): number {
  const [w, setW] = useState(0)
  useEffect(() => {
    const el = ref.current
    if (!on || !el) return
    const ro = new ResizeObserver(([e]) => setW(e!.contentRect.width))
    ro.observe(el)
    return () => ro.disconnect()
  }, [ref, on])
  return w
}
