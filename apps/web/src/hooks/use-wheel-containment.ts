import { useCallback, useRef } from "react"

// EXP-619: the agent dock is an IDE-style terminal that floats over a page
// which scrolls behind it, so a wheel gesture the dock cannot swallow ends up
// moving that page instead — the feed sticks to its bottom edge, so scrolling
// down over the terminal almost always slid the issue list underneath it.
//
// `overscroll-behavior: contain` fixes only half of that: browsers consult it
// on boxes that actually have scrollable overflow, so a short feed (a session
// that just started, a two-line run) or the dock's own chrome — header, tab
// strip, composer — still chains straight through to the document. This hook
// is the same rule applied by hand: a wheel event over the dock is allowed
// through only when some scroller between the pointer and the dock root still
// has room to move in that direction; otherwise it is swallowed.
//
// The listener is non-passive on purpose (it calls `preventDefault`) and is
// attached to the panel only, never to the bare tab strip — that strip sits at
// the bottom edge of every page, and killing the wheel there would create a
// dead band over content the user is only reading.

const SCROLLABLE_OVERFLOW = /(auto|scroll|overlay)/

// Sub-pixel scroll offsets (zoom, fractional layout) mean the bottom edge is
// never exactly `scrollHeight - clientHeight`.
const EDGE_TOLERANCE_PX = 1

function hasRoom(
  position: number,
  visible: number,
  total: number,
  delta: number
): boolean {
  if (total - visible <= EDGE_TOLERANCE_PX) return false
  return delta < 0
    ? position > EDGE_TOLERANCE_PX
    : position + visible < total - EDGE_TOLERANCE_PX
}

function consumesWheel(element: Element, deltaX: number, deltaY: number) {
  const style = getComputedStyle(element)
  if (
    deltaY !== 0 &&
    SCROLLABLE_OVERFLOW.test(style.overflowY) &&
    hasRoom(
      element.scrollTop,
      element.clientHeight,
      element.scrollHeight,
      deltaY
    )
  ) {
    return true
  }
  return (
    deltaX !== 0 &&
    SCROLLABLE_OVERFLOW.test(style.overflowX) &&
    hasRoom(
      element.scrollLeft,
      element.clientWidth,
      element.scrollWidth,
      deltaX
    )
  )
}

/**
 * True when the wheel event can still move something between `target` and
 * `root` (inclusive). Exported for the unit test — prefer the hook.
 */
export function wheelHasSomewhereToGo(
  target: EventTarget | null,
  root: Element,
  deltaX: number,
  deltaY: number
): boolean {
  let node =
    target instanceof Element
      ? target
      : target instanceof Node
        ? target.parentElement
        : null
  while (node) {
    if (consumesWheel(node, deltaX, deltaY)) return true
    if (node === root) return false
    node = node.parentElement
  }
  // The target left the tree mid-gesture: nothing to contain.
  return true
}

/**
 * Keeps wheel scrolling inside the returned element: attach the ref to a
 * region that floats over scrollable page content. It is a callback ref
 * because the region it guards mounts and unmounts long after its owner does.
 */
export function useWheelContainment<T extends HTMLElement>() {
  const detach = useRef<(() => void) | null>(null)
  return useCallback((node: T | null) => {
    detach.current?.()
    detach.current = null
    if (!node) return
    const onWheel = (event: WheelEvent) => {
      // ctrl+wheel is pinch-zoom, never a scroll.
      if (event.ctrlKey || event.defaultPrevented) return
      if (wheelHasSomewhereToGo(event.target, node, event.deltaX, event.deltaY))
        return
      event.preventDefault()
    }
    node.addEventListener(`wheel`, onWheel, { passive: false })
    detach.current = () => node.removeEventListener(`wheel`, onWheel)
  }, [])
}
