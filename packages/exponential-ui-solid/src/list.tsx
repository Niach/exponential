// FEED-128: the WINDOWED list on ONE axis (React's list.tsx on Solid; round
// 2 §5). The numbers are the core's (`itemOffsets`, `virtualWindow`,
// `scrollOffsetForIndex`, `stickyHeader`): rows sit end to end with `gap`
// between them, unmeasured rows take the measured mean (or the estimate);
// rows are positioned absolutely inside a spacer as long as the content;
// ONE ResizeObserver corrects each rendered row's extent. The window
// follows the nearest ancestor that ACTUALLY scrolls on the axis, else the
// host viewport. A sticky header row pins at the viewport start.
// `scrollToIndex` brings a row into view minus the pinned header.
//
// Rows are KEYED by `itemKey`; `renderItem` receives the row's index as an
// ACCESSOR (a keyed row keeps its subtree while its index moves).

import { For, createEffect, createMemo, createSignal, on, onCleanup, untrack, type Accessor, type JSX } from "solid-js"
import { itemExtents, itemOffsets, scrollOffsetForIndex, stickyHeader, virtualWindow, WINDOW_OVERSCAN, type ScrollAlign } from "@exponential-at/ui"
import { listViewport as viewportOf, scrollParent, type ListAxis } from "@exponential-at/ui/web"
import { bindAttrs } from "./bind-attrs"
import { css } from "./style"

export type { ListAxis }
export { scrollParent }

export interface WindowedListProps {
  count: number
  itemKey: (index: number) => string
  /** The row's content; `index` follows the row (keyed by `itemKey`). */
  renderItem: (index: Accessor<number>) => JSX.Element
  /** The extent (px on the axis) of a row before it is measured. */
  estimatedItemHeight?: number | ((index: number) => number)
  overscan?: number
  class?: string
  axis?: ListAxis
  direction?: `ltr` | `rtl`
  /** Space between rows on the axis (px). */
  gap?: number
  /** A divider drawn centred in the gap BEFORE row `index` (null = none). */
  divider?: (index: number) => JSX.Element | null
  /** Flat indices of the header rows that pin (ascending). */
  stickyRows?: readonly number[]
  /** Extra attributes of row `index`'s wrapper (role, aria-setsize…). */
  itemProps?: (index: number) => Record<string, unknown>
  /** Receives the list's handle (React's forwarded ref). */
  ref?: (handle: WindowedListHandle) => void
}

export interface WindowedListHandle {
  /** Scroll row `index` into view (default nearest), the pinned header's
   *  extent subtracted. */
  scrollToIndex: (index: number, align?: ScrollAlign) => void
}

export function WindowedList(props: WindowedListProps): JSX.Element {
  let el: HTMLDivElement | undefined
  const extentsByKey = new Map<string, number>()
  const crossByKey = new Map<string, number>()
  const observed = new Map<string, Element>()
  const keyOfEl = new WeakMap<Element, string>()
  let observer: ResizeObserver | null = null
  const [version, bump] = createSignal(0)
  const [view, setView] = createSignal<{ scroll: number; viewport: number } | null>(null, { equals: (a, b) => a === b || (!!a && !!b && a.scroll === b.scroll && a.viewport === b.viewport) })
  const axis = () => props.axis ?? `vertical`
  const horizontal = () => axis() === `horizontal`
  const rtl = () => horizontal() && props.direction === `rtl`
  const gap = () => props.gap ?? 0
  const estimated = () => props.estimatedItemHeight ?? 40

  const extents = (): number[] => {
    const est = estimated()
    const count = props.count
    // Read once (VAPP-148): a props read per row was most of the pass.
    const key = props.itemKey
    // §5: a fixed estimate gives way to the mean of the measured rows.
    if (typeof est === `number`) return itemExtents(Array.from({ length: count }, (_, i) => extentsByKey.get(key(i))), est)
    const out = new Array<number>(count)
    for (let i = 0; i < count; i++) out[i] = extentsByKey.get(key(i)) ?? est(i)
    return out
  }

  createEffect(() => {
    const isH = horizontal()
    if (typeof ResizeObserver === `undefined`) return
    const ro = new ResizeObserver((entries) => {
      let changed = false
      for (const entry of entries) {
        const key = keyOfEl.get(entry.target)
        const box = entry.borderBoxSize?.[0]
        const w = box ? box.inlineSize : entry.contentRect.width
        const h = box ? box.blockSize : entry.contentRect.height
        const main = isH ? w : h
        const cross = isH ? h : w
        if (key === undefined || main <= 0) continue
        if (extentsByKey.get(key) !== main) {
          extentsByKey.set(key, main)
          changed = true
        }
        if (crossByKey.get(key) !== cross) {
          crossByKey.set(key, cross)
          changed = true
        }
      }
      if (changed) bump((n) => n + 1)
    })
    observer = ro
    for (const node of observed.values()) ro.observe(node)
    onCleanup(() => {
      ro.disconnect()
      observer = null
    })
  })

  // The scroller (undefined = not resolved yet, null = the window).
  const [scroller, setScroller] = createSignal<HTMLElement | null | undefined>(undefined)
  createEffect(() => {
    const s = scroller()
    const a = axis()
    const r = rtl()
    if (!el || s === undefined) return
    const node = el
    const read = () => {
      const v = viewportOf(node, a, r, s)
      setView({ scroll: v.scroll, viewport: v.viewport })
    }
    read()
    const target: EventTarget = s ?? window
    target.addEventListener(`scroll`, read, { passive: true })
    window.addEventListener(`resize`, read)
    const ro = typeof ResizeObserver === `undefined` ? null : new ResizeObserver(read)
    if (ro) {
      if (s) ro.observe(s)
      ro.observe(node)
    }
    onCleanup(() => {
      target.removeEventListener(`scroll`, read)
      window.removeEventListener(`resize`, read)
      ro?.disconnect()
    })
  })

  const ext = createMemo(() => {
    version()
    return extents()
  })
  const off = createMemo(() => itemOffsets(ext(), gap()))
  // A memo: the scroller is re-found when the TOTAL changes (React's deps),
  // not on every new offsets array (a sort, a re-keyed window).
  const total = createMemo(() => off()[props.count] ?? 0)
  createEffect(() => {
    props.count
    total()
    const a = axis()
    if (!el) return
    const found = scrollParent(el, a)
    setScroller(() => found)
  })
  const fallbackViewport = () => (typeof window !== `undefined` ? (horizontal() ? window.innerWidth : window.innerHeight) : 800)
  const scroll = () => view()?.scroll ?? 0
  const viewport = () => view()?.viewport || fallbackViewport()
  const win = createMemo(() => virtualWindow(ext(), gap(), scroll(), viewport(), props.overscan ?? WINDOW_OVERSCAN), undefined, { equals: (a, b) => a.start === b.start && a.end === b.end })
  const headers = createMemo(() => [...(props.stickyRows ?? [])].filter((r) => r >= 0 && r < props.count).sort((a, b) => a - b))
  const pinned = createMemo(() => (headers().length && scroll() >= 0 ? stickyHeader(off(), ext(), headers(), scroll()) : null), undefined, { equals: (a, b) => a === b || (!!a && !!b && a.row === b.row && a.offset === b.offset) })

  // scrollToIndex: scroll so the row sits where `align` says, minus the
  // header pinned there; re-aimed after the rows it reveals are measured.
  let pending: { index: number; align: ScrollAlign; tries: number } | null = null
  const jump = (index: number, align: ScrollAlign): boolean => {
    if (!el || index < 0 || index >= props.count) return false
    // The memo: one pass over the rows per measurement (VAPP-148, React's
    // VAPP-139); a jump and every re-aim reuse it.
    const e = ext()
    const v = viewportOf(el, axis(), rtl())
    let header = -1
    for (const r of headers()) if (r <= index) header = r
    const inset = header >= 0 && header !== index ? e[header] : 0
    const target = scrollOffsetForIndex(e, gap(), index, v.viewport, Math.max(0, v.scroll), align, inset)
    const delta = target - v.scroll
    if (Math.abs(delta) < 0.5) return false
    const by = horizontal() ? { left: rtl() ? -delta : delta } : { top: delta }
    if (v.scroller) v.scroller.scrollBy(by)
    else window.scrollBy(by)
    return true
  }
  // A measurement anywhere moves the measured mean: re-aim until input.
  createEffect(
    on([ext, view], () => {
      const p = pending
      if (!p) return
      if (p.tries++ >= 12) pending = null
      else untrack(() => jump(p.index, p.align))
    })
  )
  createEffect(() => {
    const target: EventTarget = scroller() ?? window
    const cancel = () => {
      pending = null
    }
    const events = [`wheel`, `touchstart`, `pointerdown`, `keydown`] as const
    for (const e of events) target.addEventListener(e, cancel, { passive: true })
    onCleanup(() => {
      for (const e of events) target.removeEventListener(e, cancel)
    })
  })
  props.ref?.({
    scrollToIndex: (index, align = `nearest`) => {
      pending = { index, align, tries: 0 }
      if (!untrack(() => jump(index, align))) pending = null
    },
  })

  const place = (at: number) => (horizontal() ? (rtl() ? { right: 0, transform: `translateX(${-at}px)` } : { left: 0, transform: `translateX(${at}px)` }) : { transform: `translateY(${at}px)` })
  // The painted rows: the pinned header (when outside the window) then the
  // window, by key.
  const indices = createMemo(() => {
    const w = win()
    const p = pinned()
    const out: number[] = []
    if (p && (p.row < w.start || p.row >= w.end)) out.push(p.row)
    for (let i = w.start; i < w.end; i++) out.push(i)
    return out
  })
  const rows = createMemo(() => {
    const byKey = new Map<string, number>()
    for (const i of indices()) byKey.set(props.itemKey(i), i)
    return byKey
  })
  const keys = createMemo(() => [...rows().keys()], undefined, { equals: (a, b) => a.length === b.length && a.every((k, i) => k === b[i]) })
  const cross = createMemo(() => {
    version()
    let c = 0
    for (const k of keys()) c = Math.max(c, crossByKey.get(k) ?? 0)
    return c
  })
  const spacer = () => css(horizontal() ? { width: total(), height: cross() || undefined } : { height: total() })

  return (
    <div ref={el} class={props.class ? `xui-list-window ${props.class}` : `xui-list-window`} data-axis={axis()} style={spacer()} data-window-start={win().start} data-window-end={win().end}>
      <For each={keys()}>
        {(key) => {
          const index = createMemo<number>((prev) => rows().get(key) ?? prev!)
          const isPinned = () => pinned()?.row === index()
          const at = () => (isPinned() ? pinned()!.offset : off()[index()])
          const line = createMemo(() => (index() > 0 && props.divider ? props.divider(index()) : null))
          const measure = (node: HTMLDivElement) => {
            if (props.itemProps) bindAttrs(node, () => props.itemProps?.(index()))
            observed.set(key, node)
            keyOfEl.set(node, key)
            observer?.observe(node)
            onCleanup(() => {
              if (observed.get(key) === node) observed.delete(key)
              observer?.unobserve(node)
            })
          }
          return (
            <div
              ref={measure}
              class="xui-list-item"
              data-index={index()}
              data-pinned={isPinned() && pinned()!.offset !== off()[index()] ? `` : undefined}
              style={css({ ...place(at()), ...(isPinned() ? { zIndex: 1 } : {}) })}
            >
              {line() ? (
                <div class="xui-list-gap" aria-hidden="true" data-axis={axis()} style={css(horizontal() ? { [rtl() ? `right` : `left`]: -(gap() / 2) } : { top: -(gap() / 2) })}>
                  {line()}
                </div>
              ) : null}
              {props.renderItem(index)}
            </div>
          )
        }}
      </For>
    </div>
  )
}
