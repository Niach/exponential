// FEED-200 (fixtures/swipe.json): a `swipeable` Tabs on the web, framework
// free so the React and Solid renderers share it: the touch pager (axis
// lock, a sideways-scrolling child keeps its touch, the drawn offset, the
// settle) and the paint of the pages and the header `thumb`, the twin of
// the core's `layout_tree/containers.rs` (`tab_pages`, `tab_thumb`) and
// `surface/tabs.rs` (`tab_swipe`).

import { swipeBlocks, swipeIndicator, swipeLock, swipeOffset, swipeProgress, swipeSettle, swipeStep, type SwipeLock } from "../swipe"

/** The live drag: the DRAWN offset (`swipeOffset`) and the pager width it
 *  was drawn against; null = at rest. */
export interface TabSwipeDrag {
  offset: number
  width: number
}

/** A header's box (the `tab`, the `item` on a strip) from the list's
 *  padding box; null = not laid out (a folded strip tab). */
export interface TabSpan {
  x: number
  y: number
  w: number
  h: number
}

const MOTION = `var(--xui-motion-standard) var(--xui-ease-decelerate,ease-out)`
/** How far back the release velocity looks (ms). */
const VELOCITY_WINDOW = 80

/** Whether a touch on `target` belongs to a node between it and the
 *  `pager` that scrolls, pans or drags sideways (`swipeBlocks`). */
export function swipeBlocked(target: EventTarget | null, pager: Element): boolean {
  let el = target instanceof Element ? target : null
  while (el && el !== pager) {
    const component = el.getAttribute(`data-xui-c`)
    if (component && swipeBlocks(component, el.getAttribute(`data-r-direction`), getComputedStyle(el).overflowX)) return true
    el = el.parentElement
  }
  return false
}

export interface TabSwipeSource {
  /** The active tab's array index. */
  index: number
  count: number
  rtl: boolean
}

export interface TabSwipeHandlers {
  /** The live drag, null = back to rest. */
  drag: (drag: TabSwipeDrag | null) => void
  /** A release that pages: the new tab's array index. */
  settle: (index: number) => void
}

/**
 * A pointerdown on a swipeable Tabs' pager: only a TOUCH swipes (mouse and
 * pen never page). The touch waits for its axis (`swipeLock`): vertical =
 * the page scrolls, this touch is done; horizontal = the pages follow the
 * finger (`swipeOffset`), the release settles (`swipeSettle`, the velocity
 * of the last moves); a cancel springs back.
 */
export function startTabSwipe(e: PointerEvent, pager: HTMLElement, source: () => TabSwipeSource, on: TabSwipeHandlers): void {
  if (e.pointerType !== `touch` || swipeBlocked(e.target, pager)) return
  const id = e.pointerId
  const startX = e.clientX
  const startY = e.clientY
  let lock: SwipeLock = `pending`
  let src: TabSwipeSource = source()
  let width = 0
  let samples: { t: number; x: number }[] = []
  const sample = (x: number) => {
    const t = performance.now()
    samples.push({ t, x })
    samples = samples.filter((s) => t - s.t <= VELOCITY_WINDOW)
  }
  const velocity = () => {
    const a = samples[0]
    const b = samples[samples.length - 1]
    return a && b && b.t > a.t ? (b.x - a.x) / (b.t - a.t) : 0
  }
  const move = (ev: PointerEvent) => {
    if (ev.pointerId !== id) return
    const dx = ev.clientX - startX
    if (lock === `pending`) {
      lock = swipeLock(dx, ev.clientY - startY)
      if (lock === `vertical`) return finish(null)
      if (lock === `pending`) return
      src = source()
      width = pager.clientWidth
      try {
        pager.setPointerCapture(id)
      } catch {
        // A synthetic or ended pointer: the window listeners still follow it.
      }
    }
    ev.preventDefault()
    sample(ev.clientX)
    on.drag({ offset: swipeOffset(src.index, src.count, dx, width, src.rtl), width })
  }
  const up = (ev: PointerEvent) => {
    if (ev.pointerId !== id) return
    if (ev.type !== `pointerup` || lock !== `horizontal`) return finish(null)
    sample(ev.clientX)
    finish(swipeSettle(src.index, src.count, ev.clientX - startX, velocity(), width, src.rtl))
  }
  const finish = (to: number | null) => {
    window.removeEventListener(`pointermove`, move)
    window.removeEventListener(`pointerup`, up)
    window.removeEventListener(`pointercancel`, up)
    if (lock !== `horizontal`) return
    if (to !== null && to !== src.index) on.settle(to)
    on.drag(null)
    // The release after a swipe is no click.
    window.addEventListener(`click`, swallow, { capture: true, once: true })
    setTimeout(() => window.removeEventListener(`click`, swallow, { capture: true }), 0)
  }
  window.addEventListener(`pointermove`, move)
  window.addEventListener(`pointerup`, up)
  window.addEventListener(`pointercancel`, up)
}

function swallow(e: Event) {
  e.stopPropagation()
  e.preventDefault()
}

/** The pane a panel is at: 0 = the active one, ±1 = a neighbour (drawn,
 *  inert), null = hidden. */
export function tabPage(index: number, active: number): -1 | 0 | 1 | null {
  const side = index - active
  return side === 0 || side === 1 || side === -1 ? side : null
}

/** A shown panel's style (`side` from `tabPage`): one page width to its
 *  side (mirrored under RTL: the next page sits on the left) plus the drawn
 *  offset; no motion under a finger, else the settle slides. Keys are the
 *  same in React's and Solid's style objects. */
export function tabPageStyle(side: -1 | 0 | 1, drag: TabSwipeDrag | null, rtl: boolean): { transform: string; transition: string } {
  const pct = (rtl ? -side : side) * 100
  const offset = drag?.offset ?? 0
  const transform = pct === 0 && offset === 0 ? `none` : offset === 0 ? `translateX(${pct}%)` : pct === 0 ? `translateX(${offset}px)` : `translateX(calc(${pct}% + ${offset}px))`
  return { transform, transition: drag ? `none` : `transform ${MOTION}` }
}

/** Each header's span (array index order) from the list's padding box:
 *  the list is the thumb's offset parent. */
export function tabSpans(headers: readonly (HTMLElement | null | undefined)[]): (TabSpan | null)[] {
  return headers.map((el) => (el && !el.hasAttribute(`data-folded`) ? { x: el.offsetLeft, y: el.offsetTop, w: el.offsetWidth, h: el.offsetHeight } : null))
}

/** Spans are whole px (`offset*`): equal = same JSON. */
export const sameSpans = (a: readonly (TabSpan | null)[] | null, b: readonly (TabSpan | null)[]): boolean => a !== null && JSON.stringify(a) === JSON.stringify(b)

/** The `thumb`'s style: at the active header's span, carried towards the
 *  neighbour's by the drag (`swipeIndicator`); null = not measured yet
 *  (the headers keep their own selected paint). */
export function tabThumbStyle(spans: readonly (TabSpan | null)[], active: number, drag: TabSwipeDrag | null, rtl: boolean): { transform: string; top: string; width: string; height: string; transition: string } | null {
  const from = spans[active]
  if (!from) return null
  let span = { x: from.x, w: from.w }
  if (drag) {
    const to = spans[active + swipeStep(drag.offset, rtl)]
    if (to) span = swipeIndicator(span, { x: to.x, w: to.w }, swipeProgress(drag.offset, drag.width))
  }
  return {
    transform: `translateX(${span.x}px)`,
    top: `${from.y}px`,
    width: `${span.w}px`,
    height: `${from.h}px`,
    transition: drag ? `none` : `transform ${MOTION}, width ${MOTION}`,
  }
}
