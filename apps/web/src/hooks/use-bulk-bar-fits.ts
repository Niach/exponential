import { useLayoutEffect, useRef, useState, type RefObject } from "react"

// #EXP-1872 (EXP-698's width gate, the IDE's `bulk_bar_label_min_width`): the
// floating bulk bar keeps its labels while the labelled bar fits the strip it
// centres in. The strip is THE pinned foot, so its padding already leaves the
// Assistant FAB's corner out; a bar too wide for what is left drops to its
// glyphs instead of running under the FAB. The labelled width is remembered
// while the glyphs show, so the gate never flips back and forth.
export function useBulkBarIconOnly(strip: RefObject<HTMLElement | null>, active: boolean): boolean {
  const [iconOnly, setIconOnly] = useState(false)
  const labelled = useRef(0)
  const shown = useRef(iconOnly)
  shown.current = iconOnly
  useLayoutEffect(() => {
    const node = strip.current
    if (!active || !node || typeof ResizeObserver === `undefined`) return
    const check = () => {
      const style = getComputedStyle(node)
      const room = node.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight)
      const bar = node.querySelector<HTMLElement>(`[data-testid="bulk-action-bar"]`)
      if (bar && !shown.current) labelled.current = bar.scrollWidth
      setIconOnly(labelled.current > room)
    }
    check()
    const observer = new ResizeObserver(check)
    observer.observe(node)
    const bar = node.querySelector(`[data-testid="bulk-action-bar"]`)
    if (bar) observer.observe(bar)
    return () => observer.disconnect()
  }, [strip, active])
  return iconOnly
}
