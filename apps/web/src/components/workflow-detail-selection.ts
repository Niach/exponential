// EXP-1084: the workflow page's PICKER model — pure, so it tests without a
// DOM. The node strip is the picker: `All` first (position 0), then every
// node in DAG order (`workflowNodeStrip`, waves left to right). A selection is
// a set of node ids; an EMPTY selection IS `All`.

export type WorkflowFace = `issue` | `runs` | `changes` | `results`

export const WORKFLOW_FACES: readonly WorkflowFace[] = [
  `issue`,
  `runs`,
  `changes`,
  `results`,
]

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

/** The key a keydown steps with, or null. Modified keys (shift included:
 *  shift-arrow selects text / extends elsewhere) never step. */
export function stripStepKey(event: {
  key: string
  metaKey?: boolean
  ctrlKey?: boolean
  altKey?: boolean
  shiftKey?: boolean
}): 1 | -1 | null {
  if (event.metaKey || event.ctrlKey || event.altKey || event.shiftKey) return null
  if (event.key === `ArrowRight` || event.key === `j`) return 1
  if (event.key === `ArrowLeft` || event.key === `k`) return -1
  return null
}

/**
 * What the body draws for a selection × face:
 * - `issue-list`: the picked (or every) issue as the nested list — `All`, or
 *   several nodes.
 * - `issue-detail`: exactly one node's issue, the Work screen's Issue face.
 * - `runs` / `changes` / `results`: the same body for `All` and a pick, the
 *   pick only filters it.
 */
export type WorkflowBody =
  | `issue-list`
  | `issue-detail`
  | `runs`
  | `changes`
  | `results`

export function workflowBody(face: WorkflowFace, selected: number): WorkflowBody {
  if (face === `issue`) return selected === 1 ? `issue-detail` : `issue-list`
  return face
}

/** The letter keys a DOCUMENT-level listener may step with: letters never
 *  scroll, so j/k work from anywhere that is not typing. ←/→ step only while
 *  focus is inside the strip (they scroll the diff and the transcript). */
export function isLetterStepKey(key: string): boolean {
  return key === `j` || key === `k`
}

export function parseWorkflowFace(value: unknown): WorkflowFace | undefined {
  return WORKFLOW_FACES.includes(value as WorkflowFace)
    ? (value as WorkflowFace)
    : undefined
}
