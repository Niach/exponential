// FEED-200 (fixtures/swipe.json): the pure rules of a horizontal swipe
// pager, shared by every renderer (the Rust core's `swipe.rs` replays the
// same vectors; Swift and Kotlin reach it through the FFI). `Tabs.swipeable`
// pages its panels with them; FEED-201 reuses them for app screens.
// Offsets and velocities are SCREEN px (px/ms), positive = rightwards.

export const SWIPE = {
  /** px a touch travels before it commits to an axis. */
  slop: 8,
  /** Fraction of the page width a slow drag must pass to page. */
  distance: 0.5,
  /** px/ms a release must reach (towards the drag) to page on a fling. */
  velocity: 0.4,
  /** How much of the finger's travel shows past the first/last page. */
  resistance: 0.3,
} as const

export type SwipeLock = `pending` | `horizontal` | `vertical`

/** Which axis a touch that moved (`dx`, `dy`) from its start commits to:
 *  `pending` inside the slop, `horizontal` when it moved more across than
 *  down, else `vertical` (the page scrolls, the pager never starts). */
export function swipeLock(dx: number, dy: number): SwipeLock {
  const ax = Math.abs(dx)
  const ay = Math.abs(dy)
  if (Math.max(ax, ay) < SWIPE.slop) return `pending`
  return ax > ay ? `horizontal` : `vertical`
}

/** The index step a screen offset points at: dragging left reveals the
 *  next page (the previous one under RTL); 0 = none. */
export function swipeStep(offset: number, rtl = false): -1 | 0 | 1 {
  if (offset === 0) return 0
  const forward = offset < 0 ? 1 : -1
  return (rtl ? -forward : forward) as -1 | 1
}

/** The offset the pages are DRAWN at while the finger is at `offset`: the
 *  finger's travel, a fraction of it past the first/last page (no
 *  neighbour there), never more than one page. */
export function swipeOffset(index: number, count: number, offset: number, width: number, rtl = false): number {
  const target = index + swipeStep(offset, rtl)
  const damped = target < 0 || target >= count ? offset * SWIPE.resistance : offset
  const room = Math.max(width, 0)
  return Math.min(Math.max(damped, -room), room)
}

/** Which page a released swipe settles on: one step towards the drag when
 *  the release flings that way (|velocity| ≥ `SWIPE.velocity`, past the
 *  slop) or the drag passed `SWIPE.distance` of the width without being
 *  flung back; else the page it started on. Never past an end. */
export function swipeSettle(index: number, count: number, offset: number, velocity: number, width: number, rtl = false): number {
  if (count <= 0) return 0
  const from = Math.min(Math.max(index, 0), count - 1)
  const target = from + swipeStep(offset, rtl)
  if (target === from || target < 0 || target >= count) return from
  const fast = Math.abs(velocity) >= SWIPE.velocity
  const along = Math.sign(velocity) === Math.sign(offset)
  if (fast && !along) return from
  if (fast && Math.abs(offset) >= SWIPE.slop) return target
  return width > 0 && Math.abs(offset) >= width * SWIPE.distance ? target : from
}

/** How far (0…1) a drawn offset has carried the pager towards its
 *  neighbour: what a header indicator interpolates by. */
export function swipeProgress(offset: number, width: number): number {
  return width > 0 ? Math.min(Math.abs(offset) / width, 1) : 0
}

/** The header indicator's span between the active tab's (`from`) and the
 *  neighbour's (`to`) at `progress`. */
export function swipeIndicator(from: { x: number; w: number }, to: { x: number; w: number }, progress: number): { x: number; w: number } {
  const t = Math.min(Math.max(progress, 0), 1)
  return { x: from.x + (to.x - from.x) * t, w: from.w + (to.w - from.w) * t }
}

const SCROLLS_ACROSS = new Set([`Table`, `Chart`, `Carousel`, `ZoomableImage`, `Slider`, `Resizable`])

/** Whether a touch that starts inside a node of `component` (its
 *  `direction` prop, its resolved `overflowX`) belongs to that node, not to
 *  an enclosing pager: anything that scrolls, pans or drags sideways. The
 *  painter asks it for the hit node and each ancestor up to the pager. */
export function swipeBlocks(component: string, direction?: string | null, overflowX?: string | null): boolean {
  if (SCROLLS_ACROSS.has(component)) return true
  if ((component === `List` || component === `ScrollArea`) && (direction === `horizontal` || direction === `both`)) return true
  return overflowX === `scroll` || overflowX === `auto`
}
