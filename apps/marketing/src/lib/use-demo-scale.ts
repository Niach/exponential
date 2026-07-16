/* ─── Shared auto-scale hook for the product demos (IDE + web app) ───
   The demos render a fixed-size canvas and shrink it via CSS transform when
   the container is narrower than the base width. SSR-safe: the prerenderer
   renderToStrings pages embedding the demos, so this no-ops without a window
   / ResizeObserver and settles at scale 1. */
import { useEffect, useRef, useState } from "react"

export function useDemoScale(baseWidth: number) {
  const ref = useRef<HTMLDivElement | null>(null)
  const [scale, setScale] = useState(1)
  useEffect(() => {
    const el = ref.current
    if (!el || typeof ResizeObserver === `undefined`) return undefined
    const ro = new ResizeObserver((entries) => {
      const w = entries[0]?.contentRect.width ?? baseWidth
      setScale(w >= baseWidth ? 1 : Math.max(w / baseWidth, 0.3))
    })
    ro.observe(el)
    return () => ro.disconnect()
  }, [baseWidth])
  return { ref, scale }
}
