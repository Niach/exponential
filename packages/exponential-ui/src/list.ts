// Round 2 (docs/round-2-contract.md §4–5): the LIST numbers every renderer
// shares — template item keys and instance ids (duplicates and empty keys →
// `#<index>`, instance suffixes accumulate through nested templates), Table
// row keys, the virtual window over measured extents (vertical OR horizontal:
// one axis), sticky section headers and scrollToIndex. Pure, mirrored by the
// Rust core; fixtures/template-items.json and fixtures/virtual-list.json lock
// it.

import { absolutePath, readPointer } from "./dynamic"
import type { ChildTemplate, UiNode } from "./types"

import { WINDOW_OVERSCAN, WINDOW_THRESHOLD } from "./layout"
import { MAX_TEMPLATE_ITEMS } from "./limits"
export { WINDOW_THRESHOLD, WINDOW_OVERSCAN } from "./layout"

/** The instance key of every item: the value at `keyPointer` (relative to
 *  the item; objects as their JSON) — `#<index>` when that value is missing,
 *  null, `` or a DUPLICATE of an earlier item's key (more `#`s until unique);
 *  without a pointer, the index. */
export function templateItemKeys(items: readonly unknown[], keyPointer?: string): string[] {
  const seen = new Set<string>()
  return items.map((item, index) => {
    let key = String(index)
    if (keyPointer !== undefined) {
      const v = readPointer(item, keyPointer.startsWith(`/`) ? keyPointer : `/${keyPointer}`)
      const own = v === undefined || v === null || v === `` ? null : typeof v === `object` ? JSON.stringify(v) : String(v)
      key = own !== null && !seen.has(own) ? own : `#${index}`
      while (seen.has(key)) key = `#${key}`
    }
    seen.add(key)
    return key
  })
}

const escapeToken = (token: string) => token.replace(/~/g, `~0`).replace(/\//g, `~1`)

/** Table row keys: the `rowKey` FIELD (default `id`) under the same rules
 *  (`#<index>` for missing, empty or duplicate). */
export function tableRowKeys(rows: readonly unknown[], rowKey = `id`): string[] {
  return templateItemKeys(rows, `/${escapeToken(rowKey)}`)
}

/** A key as one instance-suffix segment: `~` → `~0`, `.` → `~1` (as JSON
 *  pointer escapes `/`), so dotted keys never collide across nesting levels
 *  (`a.b` + `c` vs `a` + `b.c`). */
export const instanceSegment = (key: string) => key.replace(/~/g, `~0`).replace(/\./g, `~1`)

export interface TemplateInstance {
  /** The item's key (see `templateItemKeys`). */
  key: string
  /** The item's data pointer: its scope (`/posts/3`). */
  path: string
  index: number
  /** The suffix every node of the item wears: the enclosing instance's
   *  suffix + `.<key>`, the key escaped by `instanceSegment` (`.a` for an
   *  outer item, `.a.0` inside it, `.a~1b` for key `a.b`). A node's painted
   *  id = its id + the suffix. */
  instance: string
}

/** The items a node's `template` renders in a data scope: one per array
 *  item at `template.path` (relative to `scope`), keyed by `template.key`.
 *  `instance` = the suffix of the item the template sits in (`` at the top). */
export function templateInstances(data: unknown, template: ChildTemplate, scope = ``, instance = ``): TemplateInstance[] {
  const path = absolutePath(template.path, { base: scope })
  const list = readPointer(data, path)
  if (!Array.isArray(list)) return []
  const keys = templateItemKeys(list, template.key)
  // VAPP-103: never more than `maxTemplateItems` (a renderer counts them
  // per surface; past the limit the rest is not rendered).
  return list.slice(0, MAX_TEMPLATE_ITEMS).map((_, index) => ({ key: keys[index], path: `${path}/${index}`, index, instance: `${instance}.${instanceSegment(keys[index])}` }))
}

/** A template site's key in a `TemplateBudget`: the node holding the
 *  template + the data scope it sits in. */
export const templateSiteKey = (nodeId: string, scope: string) => `${nodeId}\u0000${scope}`

export interface TemplateBudget {
  /** Items each template site renders (absent = all of them). */
  allowed: Map<string, number>
  /** The template component that hit `maxTemplateItems` first; null = none. */
  exceeded: string | null
}

/** VAPP-103: the template items ONE surface instantiates, counted against
 *  `maxTemplateItems` in the Rust layout build's order (depth-first: a
 *  node's static children, then its template items, each item's own
 *  templates before the next item); past the limit the rest is not
 *  rendered. A windowed List (more than `WINDOW_THRESHOLD` rows) mounts its
 *  rows on demand: its items are not counted up front. Iterative. */
export function templateBudget(root: UiNode, data: unknown, templateNode: (componentId: string) => UiNode | undefined): TemplateBudget {
  const allowed = new Map<string, number>()
  let left = MAX_TEMPLATE_ITEMS
  let exceeded: string | null = null
  const stack: { node: UiNode; scope: string }[] = [{ node: root, scope: `` }]
  while (stack.length) {
    const { node, scope } = stack.pop()!
    const next: { node: UiNode; scope: string }[] = []
    for (const slot of Object.values(node.slots ?? {})) next.push({ node: slot, scope })
    for (const child of node.children) next.push({ node: child, scope })
    const template = node.template
    const tpl = template ? templateNode(template.component) : undefined
    const path = template ? absolutePath(template.path, { base: scope }) : ``
    const list = template && tpl ? readPointer(data, path) : undefined
    const windowed = Array.isArray(list) && node.component === `List` && node.children.length + list.length > WINDOW_THRESHOLD
    if (template && tpl && Array.isArray(list) && !windowed) {
      const count = Math.min(list.length, left)
      allowed.set(templateSiteKey(node.id, scope), count)
      left -= count
      if (count < list.length) exceeded ??= template.component
      for (let i = 0; i < count; i++) next.push({ node: tpl, scope: `${path}/${i}` })
    }
    for (let i = next.length - 1; i >= 0; i--) stack.push(next[i]!)
  }
  return { allowed, exceeded }
}

// ---------------------------------------------------------------------------
// The virtual window (one axis)
// ---------------------------------------------------------------------------

/** Start offsets of items laid end to end on one axis with `gap` between
 *  them (a `divided` list adds `$control.hairline` to the gap; the divider
 *  is centred in it): `offsets[i]` = where item i starts, `offsets[count]` =
 *  the content length. */
export function itemOffsets(extents: readonly number[], gap = 0): number[] {
  const out = new Array<number>(extents.length + 1)
  let at = 0
  for (let i = 0; i < extents.length; i++) {
    out[i] = at
    at += extents[i] + (i < extents.length - 1 ? gap : 0)
  }
  out[extents.length] = at
  return out
}

/** Every item's extent on the list's axis: a measured item keeps its
 *  extent; an unmeasured one (`null`) takes the MEAN of the measured
 *  extents once any item is measured (the content length stays stable while
 *  rows measure in, so a fling lands on the real end), else `row` (the
 *  theme's `$control.row`). */
export function itemExtents(measured: readonly (number | null | undefined)[], row: number): number[] {
  let sum = 0
  let n = 0
  for (const m of measured) {
    if (typeof m === `number`) {
      sum += m
      n++
    }
  }
  const estimate = n ? sum / n : row
  return measured.map((m) => (typeof m === `number` ? m : estimate))
}

export interface WindowRange {
  /** First rendered item. */
  start: number
  /** One past the last rendered item. */
  end: number
  /** Space before `start` / after `end - 1` (the spacers). */
  before: number
  after: number
  /** The content length. */
  total: number
}

/** The first index in [lo, hi) whose `test` holds, for a test that is
 *  false then true along the range (binary search); `hi` when none. */
function firstWhere(lo: number, hi: number, test: (i: number) => boolean): number {
  while (lo < hi) {
    const mid = (lo + hi) >>> 1
    if (test(mid)) hi = mid
    else lo = mid + 1
  }
  return lo
}

/** The items to render for a viewport `[scroll, scroll + viewport)` on the
 *  list's axis (both relative to the list's content start; a list without a
 *  bounded size windows against its nearest scrolling ancestor or the host
 *  viewport, translated into this frame): every item that intersects it
 *  (an item ending exactly at the viewport start still counts) plus
 *  `overscan` items on each side. Unmeasured items take `itemExtents`'s
 *  estimate (the caller's extents). `offsets` = `itemOffsets(extents, gap)` when the
 *  caller caches it (recompute only when an extent changes): the window is
 *  then two binary searches, O(log n) per scroll frame. */
export function virtualWindow(extents: readonly number[], gap: number, scroll: number, viewport: number, overscan = WINDOW_OVERSCAN, offsets?: readonly number[]): WindowRange {
  const count = extents.length
  const off = offsets ?? itemOffsets(extents, gap)
  const total = off[count]
  let start = firstWhere(0, count, (i) => off[i] + extents[i] >= scroll)
  let end = firstWhere(start, count, (i) => off[i] >= scroll + viewport)
  start = Math.max(0, start - overscan)
  end = Math.min(count, end + overscan)
  if (end <= start) return { start, end: start, before: 0, after: total, total }
  const before = off[start]
  const after = total - (off[end - 1] + extents[end - 1])
  return { start, end, before, after, total }
}

export type ScrollAlign = `start` | `center` | `end` | `nearest`

/** The scroll offset that brings item `index` into a viewport of length
 *  `viewport` currently at `scroll` (the `scrollToIndex` command): `start` =
 *  its start at the viewport start (after `inset`, the pinned header's
 *  extent), `end` = its end at the viewport end, `center` = centred,
 *  `nearest` (default) = the smallest move that shows it whole (none when it
 *  already is). Clamped to `[0, max(0, total - viewport)]`. `offsets` = the
 *  cached `itemOffsets`, as for `virtualWindow`. */
export function scrollOffsetForIndex(extents: readonly number[], gap: number, index: number, viewport: number, scroll: number, align: ScrollAlign = `nearest`, inset = 0, offsets?: readonly number[]): number {
  const off = offsets ?? itemOffsets(extents, gap)
  const total = off[extents.length]
  if (index < 0 || index >= extents.length) return scroll
  const top = off[index]
  const size = extents[index]
  let next = scroll
  if (align === `start`) next = top - inset
  else if (align === `end`) next = top + size - viewport
  else if (align === `center`) next = top + size / 2 - (viewport + inset) / 2
  else if (top - inset < scroll) next = top - inset
  else if (top + size > scroll + viewport) next = top + size - viewport
  return Math.max(0, Math.min(Math.max(0, total - viewport), next))
}

// ---------------------------------------------------------------------------
// Sections and sticky headers
// ---------------------------------------------------------------------------

export interface ListSection {
  /** The `sectionBy` value its items share (as `displayString`; `` for
   *  missing). */
  value: string
  /** Index of the section's first item in the data. */
  start: number
  count: number
}

/** Consecutive template items with the same `sectionBy` value (a pointer
 *  relative to each item) form a section; the data order is kept (sort the
 *  data to group it). */
export function listSections(items: readonly unknown[], sectionBy: string): ListSection[] {
  const out: ListSection[] = []
  const pointer = sectionBy.startsWith(`/`) ? sectionBy : `/${sectionBy}`
  items.forEach((item, index) => {
    const v = readPointer(item, pointer)
    const value = v === undefined || v === null ? `` : typeof v === `object` ? JSON.stringify(v) : String(v)
    const last = out[out.length - 1]
    if (last && last.value === value) last.count++
    else out.push({ value, start: index, count: 1 })
  })
  return out
}

/** The flat ROW list a sectioned list windows: a header row before each
 *  section's items (`{header: sectionIndex}` / `{item: dataIndex}`). */
export function sectionRows(sections: readonly ListSection[]): ({ header: number } | { item: number })[] {
  const rows: ({ header: number } | { item: number })[] = []
  sections.forEach((s, i) => {
    rows.push({ header: i })
    for (let k = 0; k < s.count; k++) rows.push({ item: s.start + k })
  })
  return rows
}

/** `scrollToIndex` on a SECTIONED list: the DATA index → its flat row
 *  (`sectionRows`; -1 = absent → no move), then `scrollOffsetForIndex` over
 *  the rows' extents with `inset` = the extent of the item's own section
 *  header when headers are sticky (it pins over the item), else 0. */
export function scrollOffsetForItem(rows: readonly ({ header: number } | { item: number })[], rowExtents: readonly number[], gap: number, dataIndex: number, viewport: number, scroll: number, align: ScrollAlign = `nearest`, stickyHeaders = false, offsets?: readonly number[]): number {
  const row = rows.findIndex((r) => `item` in r && r.item === dataIndex)
  if (row < 0) return scroll
  let header = row
  while (header >= 0 && !(`header` in rows[header])) header--
  const inset = stickyHeaders && header >= 0 ? rowExtents[header] : 0
  return scrollOffsetForIndex(rowExtents, gap, row, viewport, scroll, align, inset, offsets)
}

/** The sticky header at a scroll offset: the LAST header row starting at or
 *  before `scroll` is pinned at the viewport start — pushed back by the next
 *  header as it arrives (its offset = min(scroll, nextHeaderStart −
 *  extent)). `rowOffsets` = itemOffsets of the flat rows; `headerRows` = the
 *  flat indices of the header rows, ascending. Null before the first header
 *  reaches the viewport start. Binary search: O(log headers). The pinned header is rendered even when the
 *  window does not contain it. */
export function stickyHeader(rowOffsets: readonly number[], rowExtents: readonly number[], headerRows: readonly number[], scroll: number): { row: number; offset: number } | null {
  const pinned = firstWhere(0, headerRows.length, (h) => rowOffsets[headerRows[h]] > scroll) - 1
  if (pinned < 0) return null
  const row = headerRows[pinned]
  const next = headerRows[pinned + 1]
  const extent = rowExtents[row]
  const offset = next === undefined ? scroll : Math.min(scroll, rowOffsets[next] - extent)
  return { row, offset }
}
