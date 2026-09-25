// EXP-1084: the workflow page's PICKER model — pure, mirrored ×4 (desktop
// `ui::workflow_view::Selection`, iOS/Android `WorkflowSelection`) and locked
// by the contract fixture `workflow-view.json` `selection`. The node strip is
// the picker: `All` first (position 0), then every node in DAG order
// (`workflowNodeStrip`, waves left to right). A selection is a set of node
// ids; an EMPTY selection IS `All`.
//
// The rules: a click picks exactly that node (a click on the already picked
// chip keeps it); cmd/ctrl-click toggles one node in or out (the last one
// out = All); shift-click picks the DAG-order range from the anchor (the
// last plain or toggling click) to the node; a step moves ONE position from
// the cursor (the last clicked or stepped node, else the last picked one)
// through All + the nodes, clamped at both ends, landing on one node or All;
// pruning drops nodes that left the workflow (none left = All). A modified
// key never steps.

export interface StripSelection {
  /** The picked nodes, in DAG order. Empty = `All`. */
  ids: readonly string[]
  /** Where a shift-click range starts: the last plain or toggling click. */
  anchor: string | null
  /** The node the keyboard steps from: the last one clicked or stepped to. */
  cursor: string | null
}

export const ALL_SELECTION: StripSelection = { ids: [], anchor: null, cursor: null }

export interface SelectModifiers {
  /** Cmd/Ctrl-click: add or remove one node. */
  toggle?: boolean
  /** Shift-click: the range from the anchor to this node, in DAG order. */
  extend?: boolean
}

function inOrder(ids: Iterable<string>, order: readonly string[]): string[] {
  const picked = new Set(ids)
  return order.filter((id) => picked.has(id))
}

/** A click on a node chip. `null` = the `All` chip. */
export function selectNode(
  current: StripSelection,
  id: string | null,
  order: readonly string[],
  modifiers: SelectModifiers = {}
): StripSelection {
  if (id === null || !order.includes(id)) return ALL_SELECTION
  if (modifiers.extend) {
    const anchor =
      current.anchor && order.includes(current.anchor) ? current.anchor : id
    const from = order.indexOf(anchor)
    const to = order.indexOf(id)
    const range = order.slice(Math.min(from, to), Math.max(from, to) + 1)
    return { ids: range, anchor, cursor: id }
  }
  if (modifiers.toggle) {
    const next = new Set(current.ids)
    if (!next.delete(id)) next.add(id)
    const ids = inOrder(next, order)
    if (ids.length === 0) return ALL_SELECTION
    return { ids, anchor: id, cursor: id }
  }
  return { ids: [id], anchor: id, cursor: id }
}

/** ←/→ (and j/k): one step through `All` + the nodes in DAG order, landing on
 *  exactly ONE node (or `All`). Clamped at both ends. The face is the
 *  caller's and is never touched. */
export function stepSelection(
  current: StripSelection,
  direction: 1 | -1,
  order: readonly string[]
): StripSelection {
  const cursor =
    current.ids.length === 0
      ? null
      : current.cursor && current.ids.includes(current.cursor)
        ? current.cursor
        : current.ids[current.ids.length - 1]!
  const position = cursor === null ? 0 : order.indexOf(cursor) + 1
  const next = Math.min(Math.max(position + direction, 0), order.length)
  if (next === 0) return ALL_SELECTION
  const id = order[next - 1]!
  return { ids: [id], anchor: id, cursor: id }
}

/** Drops nodes that left the workflow (a replan folded them away). */
export function pruneSelection(
  current: StripSelection,
  order: readonly string[]
): StripSelection {
  const ids = inOrder(current.ids, order)
  if (ids.length === current.ids.length) return current
  if (ids.length === 0) return ALL_SELECTION
  return {
    ids,
    anchor: current.anchor && ids.includes(current.anchor) ? current.anchor : ids[0]!,
    cursor: current.cursor && ids.includes(current.cursor) ? current.cursor : ids[0]!,
  }
}

