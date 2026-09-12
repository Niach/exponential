// EXP-818: the session TREE — a run started by another run through
// `exponential_sessions_start` carries `parent_session_id`, and every session
// list (the sidebar, the Agent page, Devices on the phones) nests it under
// its parent instead of listing it as a stranger.
//
// The rule is ONE pure function, mirrored ×4 (desktop
// `domain::session_tree::nest_sessions`, iOS `SessionTree.swift`, Android
// `SessionTree.kt`) with the same four test names:
//
// 1. The caller's order is the ROOT order — the tree never re-sorts roots.
// 2. A row is a child iff `parentSessionId` names ANOTHER row of the input;
//    a parent that is not listed (ended and pruned, another team, not synced
//    yet) leaves the child a root at depth 0.
// 3. Children follow their parent directly, oldest start first (then id, so
//    the order is total), recursively — depth grows by one per level.
// 4. A cycle (defensive; the server never writes one) breaks at the first
//    repeat: the row stays where it first appeared.

export interface TreeSession {
  id: string
  parentSessionId: string | null
  startedAt: Date | string | null
}

export interface SessionTreeRow<T> {
  session: T
  /** 0 for a root, +1 per nesting level. */
  depth: number
  /** Whether this row has at least one nested child right below it. */
  hasChildren: boolean
}

function startStamp(value: Date | string | null): number {
  if (!value) return 0
  const at = typeof value === `string` ? new Date(value) : value
  const ms = at.getTime()
  return Number.isNaN(ms) ? 0 : ms
}

/** Flatten `sessions` into the nested list order with depths. */
export function nestSessions<T extends TreeSession>(
  sessions: readonly T[]
): SessionTreeRow<T>[] {
  const ids = new Set(sessions.map((session) => session.id))
  const childrenOf = new Map<string, T[]>()
  for (const session of sessions) {
    const parent = session.parentSessionId
    if (!parent || parent === session.id || !ids.has(parent)) continue
    const list = childrenOf.get(parent) ?? []
    list.push(session)
    childrenOf.set(parent, list)
  }
  for (const list of childrenOf.values()) {
    list.sort(
      (a, b) =>
        startStamp(a.startedAt) - startStamp(b.startedAt) ||
        (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)
    )
  }
  const out: SessionTreeRow<T>[] = []
  const placed = new Set<string>()
  const visit = (session: T, depth: number) => {
    if (placed.has(session.id)) return
    placed.add(session.id)
    const children = (childrenOf.get(session.id) ?? []).filter((child) => !placed.has(child.id))
    out.push({ session, depth, hasChildren: children.length > 0 })
    for (const child of children) visit(child, depth + 1)
  }
  for (const session of sessions) {
    const parent = session.parentSessionId
    const isChild = Boolean(parent) && parent !== session.id && ids.has(parent as string)
    if (!isChild) visit(session, 0)
  }
  // Anything left is a child whose ancestry cycled without a root — keep it,
  // at depth 0, in input order.
  for (const session of sessions) visit(session, 0)
  return out
}

/** The ids of every row nested (at any depth) under `id`. */
export function descendantIds<T extends TreeSession>(
  rows: readonly SessionTreeRow<T>[],
  id: string
): string[] {
  const start = rows.findIndex((row) => row.session.id === id)
  if (start < 0) return []
  const depth = rows[start].depth
  const out: string[] = []
  for (let index = start + 1; index < rows.length; index += 1) {
    if (rows[index].depth <= depth) break
    out.push(rows[index].session.id)
  }
  return out
}

/** EXP-818: the rows a COLLAPSED set leaves visible — a row whose parent (or
 * any ancestor) is collapsed is skipped. Keyed on the flattened depths, so it
 * needs nothing but `nestSessions`' output.
 *
 * A WEB presentation helper, not part of the ×4 nesting rule above: the two
 * session lists that fold (the sidebar group and the Agent page's list) share
 * it so their twisties behave identically. */
export function visibleTreeRows<T extends TreeSession>(
  rows: readonly SessionTreeRow<T>[],
  collapsed: ReadonlySet<string>
): SessionTreeRow<T>[] {
  const out: SessionTreeRow<T>[] = []
  let hideBelow: number | null = null
  for (const row of rows) {
    if (hideBelow !== null && row.depth > hideBelow) continue
    hideBelow = null
    out.push(row)
    if (row.hasChildren && collapsed.has(row.session.id)) hideBelow = row.depth
  }
  return out
}
