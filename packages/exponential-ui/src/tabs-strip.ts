// VAPP-279 (docs/round-7-contract.md, fixtures/tabs-strip.json): the pure
// rules of a `strip` Tabs, shared by every renderer (the Rust core's
// `tabs_strip.rs` replays the same vectors).

/**
 * Which tabs get an item, in order; the rest fold into one `+N`. The IDE's
 * work-tab rule (`partitionTabs`, EXP-1334): tabs pack in order until the
 * next would not fit; the `+N` width (and its gap) is reserved only once
 * something overflows; the ACTIVE tab is committed first so it is always
 * visible; one tab always survives.
 */
export function partitionTabs(widths: readonly number[], available: number, gap: number, overflowW: number, activeIndex: number | null): number[] {
  const count = widths.length
  const room = Math.max(available, 0)
  const total = widths.reduce((sum, width) => sum + width, 0) + gap * Math.max(count - 1, 0)
  if (total <= room) return widths.map((_, index) => index)
  const budget = Math.max(room - overflowW - gap, 0)
  const visible: number[] = []
  const active = activeIndex !== null && activeIndex >= 0 && activeIndex < count ? activeIndex : null
  let used = active === null ? 0 : widths[active]
  let tabs = active === null ? 0 : 1
  for (let index = 0; index < count; index++) {
    if (index === active) {
      visible.push(index)
      continue
    }
    const next = used + widths[index] + (tabs > 0 ? gap : 0)
    if (next > budget && tabs > 0) break
    visible.push(index)
    used = next
    tabs += 1
  }
  if (active !== null && !visible.includes(active)) visible.push(active)
  if (visible.length === 0 && count > 0) visible.push(0)
  return visible.sort((a, b) => a - b)
}

/**
 * Where a dragged tab lands: `centers` = the screen x of every VISIBLE
 * tab's centre in strip order, `from` = the dragged one's slot, `delta` =
 * px the pointer moved since the drag started (screen direction). The slot
 * = how many other tabs' centres the dragged centre passed (before it in
 * reading order: left of it, right of it under RTL).
 */
export function tabDropSlot(centers: readonly number[], from: number, delta: number, rtl = false): number {
  if (from < 0 || from >= centers.length) return from
  const at = centers[from] + delta
  let slot = 0
  for (let k = 0; k < centers.length; k++) {
    if (k === from) continue
    if (rtl ? centers[k] > at : centers[k] < at) slot += 1
  }
  return slot
}

/** The keyboard move (Ctrl/Cmd+Shift+ArrowLeft/ArrowRight on a tab): the
 *  slot one place over in screen direction, or null past an end (or any
 *  other key). */
export function tabKeySlot(count: number, from: number, key: string, rtl = false): number | null {
  const step = key === `ArrowRight` ? 1 : key === `ArrowLeft` ? -1 : 0
  if (step === 0) return null
  const to = from + (rtl ? -step : step)
  return to < 0 || to >= count ? null : to
}

/** A drop slot → the `reorder` payload over the `tabs` array: `visible` =
 *  the array index of each visible slot; the dragged tab takes the array
 *  position of the tab in its target slot. Null = no move. */
export function tabReorder(visible: readonly number[], fromSlot: number, toSlot: number): { from: number; to: number } | null {
  const from = visible[fromSlot]
  const to = visible[toSlot]
  if (from === undefined || to === undefined || from === to) return null
  return { from, to }
}

/** The preview order while a drag is live: `order` (array indexes in strip
 *  order) with `from` moved to `to`'s position. */
export function moveTab<T>(order: readonly T[], from: number, to: number): T[] {
  const out = order.slice()
  if (from < 0 || from >= out.length || to < 0 || to >= out.length) return out
  const [moved] = out.splice(from, 1)
  out.splice(to, 0, moved)
  return out
}
