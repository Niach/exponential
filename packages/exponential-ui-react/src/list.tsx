// VAPP-87 + round 2 (docs/round-2-contract.md §5): the WINDOWED list on ONE
// axis (vertical or horizontal). The numbers are the core's (`itemOffsets`,
// `virtualWindow`, `scrollOffsetForIndex`, `stickyHeader` from
// `@exponential-at/ui`): rows sit end to end with `gap` between them (a
// divided list adds the hairline to the gap and draws the divider centred
// in it), unmeasured rows take the measured mean, or the estimate (`$control.row`) before any is measured. Rows are
// positioned absolutely inside a spacer as long as the content; ONE
// ResizeObserver corrects each rendered row's extent (a row leaving the
// window is unobserved, the observer disconnects on unmount). The window
// follows the nearest ancestor that ACTUALLY scrolls on the axis (overflow
// auto/scroll AND content longer than its box: the List itself when it has
// a bounded size), else the host viewport; it is re-found when the rows or
// their total change, and the viewport re-read on scroll and on a resize of
// the window, the scroller or the list. A sticky header row (`stickyRows`)
// pins at the viewport start, pushed back by the next one, and renders even
// outside the window. `scrollToIndex` brings a row into view (start | center
// | end | nearest) minus the pinned header.

import { forwardRef, useCallback, useEffect, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react"
import { itemExtents, itemOffsets, scrollOffsetForIndex, stickyHeader, virtualWindow, WINDOW_OVERSCAN, type ScrollAlign } from "@exponential-at/ui"

export type ListAxis = `vertical` | `horizontal`

export interface WindowedListProps {
  count: number
  itemKey: (index: number) => string
  renderItem: (index: number) => ReactNode
  /** The extent (px on the axis) of a row before it is measured. */
  estimatedItemHeight?: number | ((index: number) => number)
  overscan?: number
  className?: string
  axis?: ListAxis
  /** Inline direction (a horizontal list starts at the right in rtl). */
  direction?: `ltr` | `rtl`
  /** Space between rows on the axis (px; a divided list adds the hairline). */
  gap?: number
  /** A divider drawn centred in the gap BEFORE row `index` (null = none). */
  divider?: (index: number) => ReactNode | null
  /** Flat indices of the header rows that pin (ascending). */
  stickyRows?: readonly number[]
  /** Extra attributes of row `index`'s wrapper (role, aria-setsize…). */
  itemProps?: (index: number) => Record<string, unknown>
}

export interface WindowedListHandle {
  /** Scroll row `index` into view (`start` | `center` | `end` | `nearest`,
   *  default nearest), the pinned header's extent subtracted. */
  scrollToIndex: (index: number, align?: ScrollAlign) => void
}

const useIsoLayoutEffect = typeof window === `undefined` ? useEffect : useLayoutEffect

/** The nearest ancestor that scrolls on `axis`: overflow auto/scroll AND a
 *  box shorter than its content (a content-sized `overflow: auto` box is not
 *  a viewport). Null = the document scrolls (the window). */
export function scrollParent(el: HTMLElement | null, axis: ListAxis = `vertical`): HTMLElement | null {
  let cur = el?.parentElement ?? null
  while (cur && cur !== document.body && cur !== document.documentElement) {
    const cs = getComputedStyle(cur)
    const o = axis === `vertical` ? cs.overflowY : cs.overflowX
    const scrolls = axis === `vertical` ? cur.scrollHeight > cur.clientHeight + 1 : cur.scrollWidth > cur.clientWidth + 1
    if ((o === `auto` || o === `scroll`) && scrolls) return cur
    cur = cur.parentElement
  }
  return null
}

/** Where the list's content starts relative to the viewport's start on the
 *  axis (the scroll offset in list coordinates) and the viewport length. */
function viewportOf(el: HTMLElement, axis: ListAxis, rtl: boolean, known?: HTMLElement | null): { scroll: number; viewport: number; scroller: HTMLElement | null } {
  const scroller = known === undefined ? scrollParent(el, axis) : known
  const r = el.getBoundingClientRect()
  if (axis === `vertical`) {
    if (scroller) {
      const s = scroller.getBoundingClientRect()
      return { scroll: s.top + scroller.clientTop - r.top, viewport: scroller.clientHeight, scroller }
    }
    return { scroll: -r.top, viewport: window.innerHeight, scroller }
  }
  if (scroller) {
    const s = scroller.getBoundingClientRect()
    const scroll = rtl ? r.right - (s.right - scroller.clientLeft) : s.left + scroller.clientLeft - r.left
    return { scroll, viewport: scroller.clientWidth, scroller }
  }
  return { scroll: rtl ? r.right - window.innerWidth : -r.left, viewport: window.innerWidth, scroller }
}

export const WindowedList = forwardRef<WindowedListHandle, WindowedListProps>(function WindowedList(
  { count, itemKey, renderItem, estimatedItemHeight = 40, overscan = WINDOW_OVERSCAN, className, axis = `vertical`, direction = `ltr`, gap = 0, divider, stickyRows, itemProps },
  handle
) {
  const ref = useRef<HTMLDivElement>(null)
  const extentsByKey = useRef<Map<string, number>>(new Map())
  const crossByKey = useRef<Map<string, number>>(new Map())
  const observed = useRef<Map<string, Element>>(new Map())
  const keyOfEl = useRef<WeakMap<Element, string>>(new WeakMap())
  const observer = useRef<ResizeObserver | null>(null)
  const [version, bump] = useState(0)
  const [view, setView] = useState<{ scroll: number; viewport: number } | null>(null)
  const horizontal = axis === `horizontal`
  const rtl = horizontal && direction === `rtl`

  const estimate = useCallback((i: number) => (typeof estimatedItemHeight === `function` ? estimatedItemHeight(i) : estimatedItemHeight), [estimatedItemHeight])
  const extents = (): number[] => {
    // §5: a fixed estimate gives way to the mean of the measured rows.
    if (typeof estimatedItemHeight === `number`) return itemExtents(Array.from({ length: count }, (_, i) => extentsByKey.current.get(itemKey(i))), estimatedItemHeight)
    const out = new Array<number>(count)
    for (let i = 0; i < count; i++) out[i] = extentsByKey.current.get(itemKey(i)) ?? estimate(i)
    return out
  }

  useEffect(() => {
    if (typeof ResizeObserver === `undefined`) return
    const ro = new ResizeObserver((entries) => {
      let changed = false
      for (const entry of entries) {
        const key = keyOfEl.current.get(entry.target)
        const box = entry.borderBoxSize?.[0]
        const w = box ? box.inlineSize : entry.contentRect.width
        const h = box ? box.blockSize : entry.contentRect.height
        const main = horizontal ? w : h
        const cross = horizontal ? h : w
        if (key === undefined || main <= 0) continue
        if (extentsByKey.current.get(key) !== main) {
          extentsByKey.current.set(key, main)
          changed = true
        }
        if (crossByKey.current.get(key) !== cross) {
          crossByKey.current.set(key, cross)
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
  }, [horizontal])

  // The scroller (undefined = not resolved yet, null = the window). Re-found
  // when the rows or their measured total change: a box may only start
  // scrolling once its rows are measured.
  const [scroller, setScroller] = useState<HTMLElement | null | undefined>(undefined)
  // The viewport is re-read on scroll, a window resize AND a resize of the
  // scroller or the list itself (a Resizable panel, a sibling growing).
  useIsoLayoutEffect(() => {
    const el = ref.current
    if (!el || scroller === undefined) return
    const read = () => {
      const v = viewportOf(el, axis, rtl, scroller)
      setView((cur) => (cur && cur.scroll === v.scroll && cur.viewport === v.viewport ? cur : { scroll: v.scroll, viewport: v.viewport }))
    }
    read()
    const target: EventTarget = scroller ?? window
    target.addEventListener(`scroll`, read, { passive: true })
    window.addEventListener(`resize`, read)
    const ro = typeof ResizeObserver === `undefined` ? null : new ResizeObserver(read)
    if (ro) {
      if (scroller) ro.observe(scroller)
      ro.observe(el)
    }
    return () => {
      target.removeEventListener(`scroll`, read)
      window.removeEventListener(`resize`, read)
      ro?.disconnect()
    }
  }, [scroller, axis, rtl])

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

  // Extents and offsets change with the rows and their measurements, never
  // with the scroll position (a scroll step re-renders only the window).
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const ext = useMemo(extents, [count, itemKey, estimate, version])
  const off = useMemo(() => itemOffsets(ext, gap), [ext, gap])
  const total = off[count] ?? 0
  useIsoLayoutEffect(() => {
    const el = ref.current
    if (!el) return
    const found = scrollParent(el, axis)
    setScroller((cur) => (cur === found ? cur : found))
  }, [count, axis, total])
  const fallbackViewport = typeof window !== `undefined` ? (horizontal ? window.innerWidth : window.innerHeight) : 800
  const scroll = view?.scroll ?? 0
  const viewport = view?.viewport || fallbackViewport
  const win = virtualWindow(ext, gap, scroll, viewport, overscan)
  const headers = useMemo(() => [...(stickyRows ?? [])].filter((r) => r >= 0 && r < count).sort((a, b) => a - b), [stickyRows, count])
  const pinned = headers.length && scroll >= 0 ? stickyHeader(off, ext, headers, scroll) : null

  // scrollToIndex: scroll so the row sits where `align` says, minus the
  // header that will be pinned there; unmeasured rows were estimates, so the
  // jump is re-aimed after the rows it reveals are measured (a few frames).
  const pending = useRef<{ index: number; align: ScrollAlign; tries: number } | null>(null)
  const jump = (index: number, align: ScrollAlign): boolean => {
    const el = ref.current
    if (!el || index < 0 || index >= count) return false
    const e = extents()
    const v = viewportOf(el, axis, rtl)
    let header = -1
    for (const r of headers) if (r <= index) header = r
    const inset = header >= 0 && header !== index ? e[header] : 0
    const target = scrollOffsetForIndex(e, gap, index, v.viewport, Math.max(0, v.scroll), align, inset)
    const delta = target - v.scroll
    if (Math.abs(delta) < 0.5) return false
    const by = horizontal ? { left: rtl ? -delta : delta } : { top: delta }
    if (v.scroller) v.scroller.scrollBy(by)
    else window.scrollBy(by)
    return true
  }
  // A measurement anywhere moves the measured mean and so every unmeasured
  // row above the target: re-aim on the next renders, until user input.
  useEffect(() => {
    const p = pending.current
    if (!p) return
    if (p.tries++ >= 12) pending.current = null
    else jump(p.index, p.align)
  })
  useEffect(() => {
    const target: EventTarget = scroller ?? window
    const cancel = () => {
      pending.current = null
    }
    const events = [`wheel`, `touchstart`, `pointerdown`, `keydown`] as const
    for (const e of events) target.addEventListener(e, cancel, { passive: true })
    return () => {
      for (const e of events) target.removeEventListener(e, cancel)
    }
  }, [scroller])
  useImperativeHandle(
    handle,
    () => ({
      scrollToIndex: (index, align = `nearest`) => {
        pending.current = { index, align, tries: 0 }
        if (!jump(index, align)) pending.current = null
      },
    }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [count, itemKey, estimate, axis, rtl, gap, headers]
  )

  const place = (at: number): CSSProperties => (horizontal ? (rtl ? { right: 0, transform: `translateX(${-at}px)` } : { left: 0, transform: `translateX(${at}px)` }) : { transform: `translateY(${at}px)` })
  const indices: number[] = []
  if (pinned && (pinned.row < win.start || pinned.row >= win.end)) indices.push(pinned.row)
  for (let i = win.start; i < win.end; i++) indices.push(i)
  let cross = 0
  const rows = indices.map((i) => {
    const key = itemKey(i)
    cross = Math.max(cross, crossByKey.current.get(key) ?? 0)
    const isPinned = pinned?.row === i
    const at = isPinned ? pinned.offset : off[i]
    const line = i > 0 && divider ? divider(i) : null
    return (
      <div key={key} ref={measure(key)} className="xui-list-item" data-index={i} data-pinned={isPinned && pinned.offset !== off[i] ? `` : undefined} {...(itemProps?.(i) ?? {})} style={{ ...place(at), ...(isPinned ? { zIndex: 1 } : {}) }}>
        {line ? (
          <div className="xui-list-gap" aria-hidden="true" data-axis={axis} style={horizontal ? { [rtl ? `right` : `left`]: -(gap / 2) } : { top: -(gap / 2) }}>
            {line}
          </div>
        ) : null}
        {renderItem(i)}
      </div>
    )
  })
  const spacer: CSSProperties = horizontal ? { width: total, height: cross || undefined } : { height: total }
  return (
    <div ref={ref} className={className ? `xui-list-window ${className}` : `xui-list-window`} data-axis={axis} style={spacer} data-window-start={win.start} data-window-end={win.end}>
      {rows}
    </div>
  )
})
