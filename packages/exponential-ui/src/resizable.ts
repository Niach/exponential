// Round 2 (docs/round-2-contract.md §1, Resizable): the panel arithmetic
// every renderer shares, so a drag or a key press gives the same sizes
// everywhere. Sizes are PERCENTAGES of the group's main axis minus its
// handles (they sum to 100); only the two panels beside the moved handle
// change. Pure, mirrored by the Rust core; fixtures/resizable.json locks it.

/** One panel's limits (Resizable `panels[i]`), percent. */
export interface PanelLimits {
  min?: number
  max?: number
  /** Dragging below half its `min` collapses it to 0; Enter toggles. */
  collapsible?: boolean
}

import { PANEL_MIN, RESIZE_STEP } from "./layout"
export { PANEL_MIN, RESIZE_HANDLE_HIT, RESIZE_STEP } from "./layout"

const EPS = 1e-9
const clampN = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v))
/** Results are rounded to 1e-6 so every platform prints the same numbers. */
const tidy = (v: number) => {
  const r = Math.round(v * 1e6) / 1e6
  return Object.is(r, -0) ? 0 : r
}

function limitsOf(limits: readonly PanelLimits[] | undefined, i: number, count: number): { min: number; max: number; collapsible: boolean } {
  const l = limits?.[i] ?? {}
  const cap = 100 / Math.max(1, count)
  const min = clampN(typeof l.min === `number` && Number.isFinite(l.min) ? l.min : PANEL_MIN, 0, cap)
  const max = clampN(typeof l.max === `number` && Number.isFinite(l.max) ? l.max : 100, min, 100)
  return { min, max, collapsible: l.collapsible === true }
}

/** The sizes a group of `count` panels starts with: the valid given sizes
 *  (finite, ≥ 0), the missing ones sharing what is left of 100 (equal split
 *  when nothing is given), scaled to sum 100, then each clamped into its
 *  [min, max] (a collapsible panel may stay at 0) with the difference handed
 *  to the panels that still have room, in order. A `min` above 100 / count
 *  is lowered to it. */
export function normalizeSizes(sizes: readonly unknown[] | undefined, count: number, limits?: readonly PanelLimits[]): number[] {
  if (count <= 0) return []
  const given = Array.from({ length: count }, (_, i) => {
    const v = sizes?.[i]
    return typeof v === `number` && Number.isFinite(v) && v >= 0 ? v : null
  })
  const missing = given.filter((v) => v === null).length
  let out: number[]
  if (missing === count) out = given.map(() => 100 / count)
  else {
    const sum = given.reduce<number>((s, v) => s + (v ?? 0), 0)
    const rest = Math.max(0, 100 - sum)
    out = given.map((v) => v ?? rest / missing)
  }
  const total = out.reduce((s, v) => s + v, 0)
  out = total > EPS ? out.map((v) => (v * 100) / total) : out.map(() => 100 / count)
  const lim = out.map((_, i) => limitsOf(limits, i, count))
  out = out.map((v, i) => (lim[i].collapsible && v <= EPS ? 0 : clampN(v, lim[i].min, lim[i].max)))
  let diff = 100 - out.reduce((s, v) => s + v, 0)
  for (let i = 0; i < count && Math.abs(diff) > EPS; i++) {
    if (lim[i].collapsible && out[i] === 0) continue
    const room = diff > 0 ? lim[i].max - out[i] : lim[i].min - out[i]
    const take = diff > 0 ? Math.min(diff, room) : Math.max(diff, room)
    out[i] += take
    diff -= take
  }
  return out.map(tidy)
}

/** Move handle `handle` (between panels `handle` and `handle + 1`) by
 *  `delta` percentage points (positive = the first panel grows). Always
 *  computed from the sizes at the START of a drag (no drift). The move is
 *  clamped so both panels stay in their [min, max]; a collapsible panel
 *  dragged below half its min collapses to 0 (the other takes its space, if
 *  it can), and a collapsed one dragged past half its min reopens: at its
 *  min, or where the pointer is when that is further (clamped as above).
 *  Other panels never change. */
export function resizePanels(sizes: readonly number[], handle: number, delta: number, limits?: readonly PanelLimits[]): number[] {
  const out = [...sizes]
  const a = handle
  const b = handle + 1
  if (a < 0 || b >= sizes.length || !Number.isFinite(delta)) return out.map(tidy)
  const la = limitsOf(limits, a, sizes.length)
  const lb = limitsOf(limits, b, sizes.length)
  const lo = Math.max(la.min - sizes[a], sizes[b] - lb.max)
  const hi = Math.min(la.max - sizes[a], sizes[b] - lb.min)
  let d = clampN(delta, Math.min(lo, 0), Math.max(hi, 0))
  // Collapsing and reopening.
  if (la.collapsible && delta < 0 && sizes[a] > 0 && sizes[a] + delta < la.min / 2 && sizes[b] + sizes[a] <= lb.max + EPS) d = -sizes[a]
  else if (lb.collapsible && delta > 0 && sizes[b] > 0 && sizes[b] - delta < lb.min / 2 && sizes[a] + sizes[b] <= la.max + EPS) d = sizes[b]
  // A collapsed panel dragged past half its min reopens at its min at least,
  // then follows the pointer (up to its max and the neighbour's min).
  else if (la.collapsible && sizes[a] === 0) d = delta >= la.min / 2 && hi >= la.min - EPS ? clampN(delta, la.min, hi) : 0
  else if (lb.collapsible && sizes[b] === 0) d = -delta >= lb.min / 2 && -lo >= lb.min - EPS ? -clampN(-delta, lb.min, -lo) : 0
  out[a] = sizes[a] + d
  out[b] = sizes[b] - d
  return out.map(tidy)
}

export type ResizeKey = `ArrowLeft` | `ArrowRight` | `ArrowUp` | `ArrowDown` | `Home` | `End` | `Enter`

/** A key on a focused handle. Arrows move the HANDLE on screen by
 *  RESIZE_STEP (horizontal group: Left/Right — in rtl the first panel sits
 *  on the right, so ArrowRight shrinks it; vertical group: Up/Down); Home /
 *  End take the first panel to its min / max (as far as the second allows);
 *  Enter collapses the first panel when collapsible (else the second),
 *  reopening a collapsed one at its min. Other keys change nothing. */
export function keyboardResize(sizes: readonly number[], handle: number, key: ResizeKey | string, orientation: `horizontal` | `vertical`, direction: `ltr` | `rtl` = `ltr`, limits?: readonly PanelLimits[]): number[] {
  const a = handle
  const b = handle + 1
  if (a < 0 || b >= sizes.length) return sizes.map(tidy)
  const la = limitsOf(limits, a, sizes.length)
  const lb = limitsOf(limits, b, sizes.length)
  const flip = orientation === `horizontal` && direction === `rtl` ? -1 : 1
  const step = (sign: number) => resizePanels(sizes, handle, sign * RESIZE_STEP, limits)
  switch (key) {
    case `ArrowRight`:
      return orientation === `horizontal` ? step(flip) : sizes.map(tidy)
    case `ArrowLeft`:
      return orientation === `horizontal` ? step(-flip) : sizes.map(tidy)
    case `ArrowDown`:
      return orientation === `vertical` ? step(1) : sizes.map(tidy)
    case `ArrowUp`:
      return orientation === `vertical` ? step(-1) : sizes.map(tidy)
    case `Home`:
      return resizePanels(sizes, handle, la.min - sizes[a], limits)
    case `End`:
      return resizePanels(sizes, handle, la.max - sizes[a], limits)
    case `Enter`: {
      const out = [...sizes]
      if (la.collapsible) {
        const d = sizes[a] > 0 ? -sizes[a] : Math.min(la.min, sizes[b] - lb.min)
        if (sizes[b] - d > lb.max + EPS || d === 0) return sizes.map(tidy)
        out[a] += d
        out[b] -= d
      } else if (lb.collapsible) {
        const d = sizes[b] > 0 ? sizes[b] : -Math.min(lb.min, sizes[a] - la.min)
        if (sizes[a] + d > la.max + EPS || d === 0) return sizes.map(tidy)
        out[a] += d
        out[b] -= d
      }
      return out.map(tidy)
    }
    default:
      return sizes.map(tidy)
  }
}

/** Panel lengths in px on the main axis: the container minus every handle
 *  (`handleExtent` each, `$control.hairline`), shared by the sizes. */
export function panelExtents(sizes: readonly number[], container: number, handleExtent = 1): number[] {
  const avail = Math.max(0, container - Math.max(0, sizes.length - 1) * handleExtent)
  return sizes.map((s) => tidy((avail * s) / 100))
}

/** A pointer movement (px on the main axis, screen direction) as the
 *  percentage delta `resizePanels` takes (rtl horizontal groups flip it). */
export function dragDelta(px: number, container: number, panelCount: number, orientation: `horizontal` | `vertical`, direction: `ltr` | `rtl` = `ltr`, handleExtent = 1): number {
  const avail = Math.max(1, container - Math.max(0, panelCount - 1) * handleExtent)
  const flip = orientation === `horizontal` && direction === `rtl` ? -1 : 1
  return tidy(((px * 100) / avail) * flip)
}
