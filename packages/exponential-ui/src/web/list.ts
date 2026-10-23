// VAPP-87 + round 2 §5 (FEED-128: hoisted from the React renderer's
// list.tsx): the DOM measurements a WINDOWED list needs, framework-free.
// The window numbers themselves are the core's (`itemOffsets`,
// `virtualWindow`, `scrollOffsetForIndex`, `stickyHeader`).

export type ListAxis = `vertical` | `horizontal`

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
 *  axis (the scroll offset in list coordinates) and the viewport length.
 *  `known` = the scroller already found (null = the window). */
export function listViewport(el: HTMLElement, axis: ListAxis, rtl: boolean, known?: HTMLElement | null): { scroll: number; viewport: number; scroller: HTMLElement | null } {
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
