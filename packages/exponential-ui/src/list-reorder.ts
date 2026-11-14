// EXP-1832 (fixtures/list-reorder.json): the pure rules of a `reorderable`
// List, shared by every renderer (the Rust core's `list_reorder.rs` replays
// the same vectors; the natives reach it through the FFI). A List item may
// carry `depth` (0 = top) and `group` (true = a row that holds rows, e.g. a
// folder); `maxDepth` caps how deep a drop may nest.

/** One RENDERED row of the list, in list order: its key, its box along the
 *  list's axis (`top`, `height`, in px), its `depth` and whether it holds
 *  rows (`group`). */
export interface ListReorderRow {
  key: string
  top: number
  height: number
  depth?: number
  group?: boolean
}

export type ListDropPosition = `before` | `after` | `into`

export interface ListDrop {
  /** The row the drop is relative to. */
  key: string
  position: ListDropPosition
}

/** The fraction of a row's height at each end that means "beside" when the
 *  row also accepts a drop INTO it (the middle half = into). */
export const LIST_DROP_EDGE = 0.25

const depthOf = (row: ListReorderRow) => Math.max(0, Math.floor(row.depth ?? 0))

/** The dragged row's subtree: itself + the rows right after it that sit
 *  deeper (its rendered children). Returns [start, endExclusive]. */
function subtree(rows: readonly ListReorderRow[], index: number): [number, number] {
  const depth = depthOf(rows[index])
  let end = index + 1
  while (end < rows.length && depthOf(rows[end]) > depth) end += 1
  return [index, end]
}

/** How many levels the dragged subtree spans below its own row: 1 for a
 *  `group` row (even folded), else the deepest rendered descendant. */
function heightOf(rows: readonly ListReorderRow[], index: number): number {
  const [start, end] = subtree(rows, index)
  let height = rows[index].group ? 1 : 0
  for (let k = start + 1; k < end; k++) height = Math.max(height, depthOf(rows[k]) - depthOf(rows[index]))
  return height
}

/**
 * Where a drag over the list drops: `rows` = the rendered rows in order,
 * `dragKey` = the dragged row, `at` = the pointer's position along the
 * list's axis (same space as `top`), `maxDepth` = nesting levels a drop may
 * make (1 = flat reorder only). Null = no drop here (the pointer is over the
 * dragged row or its own children, or the drop would nest too deep).
 *
 * The row under the pointer (above the first = the first, below the last =
 * the last) takes the drop. When the dragged subtree fits INTO it
 * (`depth + 1 + height < maxDepth`), its outer quarters mean before / after
 * and its middle half means into; otherwise its upper half = before, lower
 * half = after. Beside needs `depth + height < maxDepth`. `after` an open
 * group (its next row is its child) is `before` that child.
 */
export function listDropSlot(rows: readonly ListReorderRow[], dragKey: string, at: number, maxDepth = 1): ListDrop | null {
  const dragIndex = rows.findIndex((row) => row.key === dragKey)
  if (dragIndex < 0 || rows.length === 0) return null
  let index = rows.findIndex((row) => at >= row.top && at < row.top + row.height)
  if (index < 0) index = at < rows[0].top ? 0 : rows.length - 1
  const [start, end] = subtree(rows, dragIndex)
  if (index >= start && index < end) return null
  const row = rows[index]
  const height = heightOf(rows, dragIndex)
  const depth = depthOf(row)
  const into = depth + 1 + height < maxDepth
  const frac = row.height > 0 ? (at - row.top) / row.height : 0
  let position: ListDropPosition
  if (into) position = frac < LIST_DROP_EDGE ? `before` : frac >= 1 - LIST_DROP_EDGE ? `after` : `into`
  else position = frac < 0.5 ? `before` : `after`
  if (position === `into`) return { key: row.key, position }
  let target = row
  if (position === `after` && row.group && index + 1 < rows.length && depthOf(rows[index + 1]) > depth) {
    target = rows[index + 1]
    position = `before`
    if (index + 1 >= start && index + 1 < end) return null
  }
  if (depthOf(target) + height >= maxDepth) return null
  return { key: target.key, position }
}

/** The `move` event's context for a drop: `{id, before | after | into}`. */
export type ListMoveContext = { id: string } & Partial<Record<ListDropPosition, string>>

export function listMoveContext(dragKey: string, drop: ListDrop): ListMoveContext {
  return { id: dragKey, [drop.position]: drop.key }
}
