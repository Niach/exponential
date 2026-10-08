// VAPP-87: the WINDOWED list — the core's API shape (item key, estimated
// height, overscan), no dependency. Items are positioned absolutely inside a
// spacer sized by measured + estimated heights; a ResizeObserver corrects
// each rendered item's estimate, and the window follows the nearest
// scrolling ancestor (the List itself when it has a bounded height).

import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react"

export interface WindowedListProps {
  count: number
  itemKey: (index: number) => string
  renderItem: (index: number) => ReactNode
  estimatedItemHeight?: number
  overscan?: number
  className?: string
}

const useIsoLayoutEffect = typeof window === `undefined` ? useEffect : useLayoutEffect

function scrollParent(el: HTMLElement | null): HTMLElement | null {
  let cur = el?.parentElement ?? null
  while (cur) {
    const o = getComputedStyle(cur).overflowY
    if (o === `auto` || o === `scroll`) return cur
    cur = cur.parentElement
  }
  return null
}

export function WindowedList({ count, itemKey, renderItem, estimatedItemHeight = 40, overscan = 5, className }: WindowedListProps) {
  const ref = useRef<HTMLDivElement>(null)
  const heights = useRef<Map<string, number>>(new Map())
  const [, bump] = useState(0)
  const [viewport, setViewport] = useState({ top: 0, height: 0 })

  const offsets = (): number[] => {
    const out = new Array<number>(count + 1)
    let y = 0
    for (let i = 0; i < count; i++) {
      out[i] = y
      y += heights.current.get(itemKey(i)) ?? estimatedItemHeight
    }
    out[count] = y
    return out
  }

  useIsoLayoutEffect(() => {
    const el = ref.current
    if (!el) return
    const scroller = scrollParent(el)
    const read = () => {
      const rect = el.getBoundingClientRect()
      if (scroller) {
        const s = scroller.getBoundingClientRect()
        setViewport({ top: Math.max(0, s.top - rect.top), height: s.height })
      } else {
        setViewport({ top: Math.max(0, -rect.top), height: window.innerHeight })
      }
    }
    read()
    const target: EventTarget = scroller ?? window
    target.addEventListener(`scroll`, read, { passive: true })
    window.addEventListener(`resize`, read)
    return () => {
      target.removeEventListener(`scroll`, read)
      window.removeEventListener(`resize`, read)
    }
  }, [count])

  const measure = useCallback(
    (key: string) => (node: HTMLDivElement | null) => {
      if (!node || typeof ResizeObserver === `undefined`) return
      const ro = new ResizeObserver(([entry]) => {
        const h = entry.contentRect.height
        if (h > 0 && heights.current.get(key) !== h) {
          heights.current.set(key, h)
          bump((n) => n + 1)
        }
      })
      ro.observe(node)
    },
    []
  )

  const off = offsets()
  const total = off[count]
  const viewportHeight = viewport.height || (typeof window !== `undefined` ? window.innerHeight : 800)
  let start = 0
  while (start < count && off[start + 1] < viewport.top) start++
  let end = start
  while (end < count && off[end] < viewport.top + viewportHeight) end++
  start = Math.max(0, start - overscan)
  end = Math.min(count, end + overscan)
  const rows: ReactNode[] = []
  for (let i = start; i < end; i++) {
    const key = itemKey(i)
    rows.push(
      <div key={key} ref={measure(key)} className="xui-list-item" data-index={i} style={{ transform: `translateY(${off[i]}px)` }}>
        {renderItem(i)}
      </div>
    )
  }
  return (
    <div ref={ref} className={className ? `xui-list-window ${className}` : `xui-list-window`} style={{ height: total }} data-window-start={start} data-window-end={end}>
      {rows}
    </div>
  )
}
