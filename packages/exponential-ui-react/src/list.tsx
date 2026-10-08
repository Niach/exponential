// VAPP-87 + round 1: the WINDOWED list — the core's API shape (item key,
// estimated height, overscan), no dependency. Items are positioned
// absolutely inside a spacer sized by measured + estimated heights; ONE
// ResizeObserver corrects each rendered item's estimate (an item leaving
// the window is unobserved, the observer disconnects on unmount), and the
// window follows the nearest ancestor that ACTUALLY scrolls (overflow
// auto/scroll AND content taller than its box: the List itself when it has a
// bounded height), else the window. Every List wears `overflow: auto`, so an
// unbounded one (as tall as its spacer) is not a scroller and the page's
// scroll drives the window. `scrollToIndex` brings an item into view (the
// `scrollIntoView` host command uses the same).

import { forwardRef, useCallback, useEffect, useImperativeHandle, useLayoutEffect, useRef, useState, type ReactNode } from "react"

export interface WindowedListProps {
  count: number
  itemKey: (index: number) => string
  renderItem: (index: number) => ReactNode
  estimatedItemHeight?: number
  overscan?: number
  className?: string
}

export interface WindowedListHandle {
  /** Scroll the item at `index` into view (`start` | `center` | `end` |
   *  `nearest`, default nearest). */
  scrollToIndex: (index: number, align?: `start` | `center` | `end` | `nearest`) => void
}

const useIsoLayoutEffect = typeof window === `undefined` ? useEffect : useLayoutEffect

/** The nearest ancestor that scrolls: overflow-y auto/scroll AND a box
 *  shorter than its content (a content-sized `overflow: auto` box is not a
 *  viewport). Null = the document scrolls (the window). */
export function scrollParent(el: HTMLElement | null): HTMLElement | null {
  let cur = el?.parentElement ?? null
  while (cur && cur !== document.body && cur !== document.documentElement) {
    const o = getComputedStyle(cur).overflowY
    if ((o === `auto` || o === `scroll`) && cur.scrollHeight > cur.clientHeight + 1) return cur
    cur = cur.parentElement
  }
  return null
}

export const WindowedList = forwardRef<WindowedListHandle, WindowedListProps>(function WindowedList({ count, itemKey, renderItem, estimatedItemHeight = 40, overscan = 5, className }, handle) {
  const ref = useRef<HTMLDivElement>(null)
  const heights = useRef<Map<string, number>>(new Map())
  const observed = useRef<Map<string, Element>>(new Map())
  const keyOfEl = useRef<WeakMap<Element, string>>(new WeakMap())
  const observer = useRef<ResizeObserver | null>(null)
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

  useEffect(() => {
    if (typeof ResizeObserver === `undefined`) return
    const ro = new ResizeObserver((entries) => {
      let changed = false
      for (const entry of entries) {
        const key = keyOfEl.current.get(entry.target)
        const h = entry.contentRect.height
        if (key !== undefined && h > 0 && heights.current.get(key) !== h) {
          heights.current.set(key, h)
          changed = true
        }
      }
      if (changed) bump((n) => n + 1)
    })
    observer.current = ro
    for (const el of observed.current.values()) ro.observe(el)
    return () => {
      ro.disconnect()
      observer.current = null
    }
  }, [])

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
      const prev = observed.current.get(key)
      if (prev && prev !== node) {
        observer.current?.unobserve(prev)
        observed.current.delete(key)
      }
      if (!node) return
      observed.current.set(key, node)
      keyOfEl.current.set(node, key)
      observer.current?.observe(node)
    },
    []
  )

  useImperativeHandle(
    handle,
    () => ({
      scrollToIndex: (index, align = `nearest`) => {
        const el = ref.current
        if (!el || index < 0 || index >= count) return
        const off = offsets()
        const top = off[index]
        const h = off[index + 1] - top
        const scroller = scrollParent(el)
        const base = scroller ? el.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop : el.getBoundingClientRect().top + window.scrollY
        const vh = scroller ? scroller.clientHeight : window.innerHeight
        const cur = scroller ? scroller.scrollTop : window.scrollY
        const itemTop = base + top
        let next = cur
        if (align === `start`) next = itemTop
        else if (align === `end`) next = itemTop + h - vh
        else if (align === `center`) next = itemTop + h / 2 - vh / 2
        else if (itemTop < cur) next = itemTop
        else if (itemTop + h > cur + vh) next = itemTop + h - vh
        if (scroller) scroller.scrollTop = next
        else window.scrollTo({ top: next })
      },
    }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [count, itemKey, estimatedItemHeight]
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
})
