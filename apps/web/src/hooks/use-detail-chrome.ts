import { useCallback, useEffect, useRef, useState } from "react"
import { isTitleCollapsed } from "@/lib/detail-chrome"

// EXP-1162: the two measurements the detail chrome needs, neither of them a
// scroll listener — a header band's height (content runs UNDER an overlay
// band, so its scroller pads by it) and whether the title row has scrolled
// away under that band (an IntersectionObserver against the scroller).

/** The measured box of the node the returned ref lands on (rounded px). */
export function useMeasuredSize(): [
  (node: HTMLElement | null) => void,
  { width: number; height: number },
] {
  const [size, setSize] = useState({ width: 0, height: 0 })
  const cleanupRef = useRef<(() => void) | null>(null)
  const ref = useCallback((node: HTMLElement | null) => {
    cleanupRef.current?.()
    cleanupRef.current = null
    if (!node) return
    const read = () => {
      const rect = node.getBoundingClientRect()
      const next = {
        width: Math.round(rect.width),
        height: Math.round(rect.height),
      }
      setSize((prev) =>
        prev.width === next.width && prev.height === next.height ? prev : next
      )
    }
    read()
    if (typeof ResizeObserver === `undefined`) return
    const observer = new ResizeObserver(read)
    observer.observe(node)
    cleanupRef.current = () => observer.disconnect()
  }, [])
  return [ref, size]
}

/**
 * Whether the title row scrolled away under the header band
 * (`isTitleCollapsed`). `headerHeight` is the band's height from the
 * scroller's top edge; the scroller and the title row come back as refs.
 */
export function useTitleCollapsed(headerHeight: number): {
  scrollRef: (node: HTMLElement | null) => void
  titleRef: (node: HTMLElement | null) => void
  collapsed: boolean
} {
  const [scroller, setScroller] = useState<HTMLElement | null>(null)
  const [title, setTitle] = useState<HTMLElement | null>(null)
  const [collapsed, setCollapsed] = useState(false)

  useEffect(() => {
    if (!scroller || !title || typeof IntersectionObserver === `undefined`) {
      setCollapsed(false)
      return
    }
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (!entry?.rootBounds) return
        setCollapsed(
          isTitleCollapsed(
            true,
            entry.boundingClientRect.bottom,
            entry.rootBounds.top
          )
        )
      },
      { root: scroller, rootMargin: `-${headerHeight}px 0px 0px 0px` }
    )
    observer.observe(title)
    return () => observer.disconnect()
  }, [scroller, title, headerHeight])

  return { scrollRef: setScroller, titleRef: setTitle, collapsed }
}
