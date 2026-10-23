/* A concept draft drawn WITH Exponential UI: the post ships the draft as an
   A2UI tree (src/blog/surfaces.ts), the prerender and the first client
   render show its still (public/blog/<id>.webp, captured from this very
   render by scripts/blog-stills.ts), and once the figure nears the viewport
   the React renderer loads in its own chunk and paints the live surface in
   place. No page chunk carries the renderer. */
import { useEffect, useRef, useState, type ComponentType, type ReactNode } from "react"
import type { Nested } from "../../blog/a2ui"

export type SurfaceRuntimeProps = { id: string; tree: Nested }
type Runtime = ComponentType<SurfaceRuntimeProps>

let runtime: Promise<Runtime> | null = null
const loadRuntime = () => (runtime ??= import("./surface-runtime").then((m) => m.SurfaceRuntime))

export type LiveSurfaceProps = {
  id: string
  tree: Nested
  still: { src: string; alt: string; width: number; height: number }
  caption?: ReactNode
}

export function LiveSurface({ id, tree, still, caption }: LiveSurfaceProps) {
  const ref = useRef<HTMLElement>(null)
  const [Live, setLive] = useState<Runtime | null>(null)

  useEffect(() => {
    const el = ref.current
    if (!el) return
    let cancelled = false
    const io = new IntersectionObserver(
      (entries) => {
        if (!entries.some((e) => e.isIntersecting)) return
        io.disconnect()
        loadRuntime().then((m) => !cancelled && setLive(() => m))
      },
      { rootMargin: `400px` }
    )
    io.observe(el)
    return () => {
      cancelled = true
      io.disconnect()
    }
  }, [])

  return (
    <figure ref={ref} className="docs-shot blog-figure blog-surface" data-surface={id} data-live={Live ? `ready` : `still`}>
      <div className="blog-figure-frame blog-surface-frame" style={Live ? undefined : { aspectRatio: `${still.width} / ${still.height}` }}>
        {Live ? <Live id={id} tree={tree} /> : <img src={still.src} alt={still.alt} width={still.width} height={still.height} loading="lazy" decoding="async" />}
        <span className="blog-concept">concept · drawn with Exponential UI</span>
      </div>
      {caption ? <figcaption className="docs-embed-caption">{caption}</figcaption> : null}
    </figure>
  )
}
