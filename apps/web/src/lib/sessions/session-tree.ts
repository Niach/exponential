// EXP-1029 contract — the session tree (EXP-996 implements).
//
// ONE selector over the synced `coding_sessions` rows that every sessions
// list draws from: the web sidebar's Running section + `session-tree.tsx`,
// the IDE `sidebar.rs` Running section and its sessions list, the mobile
// sessions screens. Rules, in this order:
//
//   1. Resumed runs COLLAPSE: every resume succession (`runChain`, EXP-974)
//      is ONE node, keyed by its newest row, `chain` oldest-first.
//   2. Children nest under their `parentSessionId` (a `sessions_start`
//      child, EXP-679/818), following the parent's resume succession.
//   3. Top-level nodes sort by last activity, newest first; children keep
//      creation order.
//   4. An orphan child whose parent is gone (swept, or not synced) sits at
//      top level.
//
// Mirrored ×4 with these test names: desktop `domain::session_tree`, iOS
// `SessionTree.swift`, Android `SessionTree.kt`.
import type { CodingSession } from "@/db/schema"
import { runChain, type SessionRow as RunChainRow } from "./run-chain"

export type { RunChainRow }

/** The row fields the tree reads — a `Pick` of the synced `coding_sessions`
 *  row (plus `runChain`'s). */
export type SessionTreeRow = RunChainRow &
  Pick<
    CodingSession,
    | `parentSessionId`
    | `issueId`
    | `batchIssueIds`
    | `startedReason`
    | `status`
    | `updatedAt`
  >

export interface SessionNode<T extends SessionTreeRow = SessionTreeRow> {
  kind: `session`
  /** The newest row of the resume succession — the node's identity. */
  session: T
  /** The succession oldest-first (`runChain`); `[session]` when unresumed. */
  chain: readonly T[]
  children: SessionTreeNode<T>[]
  /** The newest `updatedAt` across the chain and the children, ms. */
  lastActivityAt: number
}

export type SessionTreeNode<T extends SessionTreeRow = SessionTreeRow> =
  SessionNode<T>

/** A row is LIVE until the server ends it (`running` and `in_review` both
 *  are; `needs_input`/`blocked` are flags on a live row). */
export function sessionRowIsLive(row: { status: string }): boolean {
  return row.status !== `ended`
}

/**
 * The sessions list as a tree. Pure: no clock, no IO; sort ties break on
 * id so two clients agree.
 */
export function sessionTree<T extends SessionTreeRow>(
  sessions: readonly T[]
): SessionTreeNode<T>[] {
  // 1. Resume successions collapse. Oldest row first, so the primary
  //    succession (`runChain`'s newest-successor walk) claims its members
  //    before an older fork sibling does; whatever is left becomes its own
  //    node. Without a fork this is exactly `runChain`.
  const ordered = [...sessions].sort(
    (a, b) => stamp(a.createdAt) - stamp(b.createdAt) || compare(a.id, b.id)
  )
  const canonicalOf = new Map<string, string>()
  const chainOf = new Map<string, T[]>()
  for (const row of ordered) {
    if (canonicalOf.has(row.id)) continue
    const chain = runChain(sessions, row.id).filter(
      (member) => !canonicalOf.has(member.id)
    )
    const canonical = chain[chain.length - 1] ?? row
    for (const member of chain) canonicalOf.set(member.id, canonical.id)
    chainOf.set(canonical.id, chain.length > 0 ? chain : [row])
  }

  // 2. Children nest under their parent's SUCCESSION (EXP-906: a resume
  //    inherits `parentSessionId`, so the whole chain answers for it).
  const parentOf = new Map<string, string>()
  for (const [canonicalId, chain] of chainOf) {
    let named: string | null = null
    for (let index = chain.length - 1; index >= 0 && !named; index -= 1) {
      named = chain[index]!.parentSessionId ?? null
    }
    const parent = named ? canonicalOf.get(named) : undefined
    // Rule 4: a parent that is gone (swept, another team, not synced) leaves
    // the child at top level; so does a row naming itself.
    if (!parent || parent === canonicalId) continue
    parentOf.set(canonicalId, parent)
  }

  const nodes = new Map<string, SessionNode<T>>()
  for (const [canonicalId, chain] of chainOf) {
    const session = chain[chain.length - 1]!
    nodes.set(canonicalId, {
      kind: `session`,
      session,
      chain,
      children: [],
      lastActivityAt: Math.max(...chain.map((row) => stamp(row.updatedAt)), 0),
    })
  }

  // A cycle (never written by the server, but a synced row is a synced row)
  // leaves the row it closes on at top level.
  const roots: SessionNode<T>[] = []
  for (const [canonicalId, node] of nodes) {
    const parentId = ancestor(canonicalId, parentOf, nodes)
    const parent = parentId ? nodes.get(parentId) : undefined
    if (parent && parent !== node) parent.children.push(node)
    else roots.push(node)
  }

  // 3. Children keep CREATION order; a parent's activity counts its
  //    subtree's, so folding one never moves it. Top-level nodes sort by
  //    last activity, newest first.
  for (const node of nodes.values()) node.children.sort(byCreation)
  for (const node of roots) rollUp(node)
  return roots.sort(
    (a, b) =>
      b.lastActivityAt - a.lastActivityAt ||
      compare(sessionTreeNodeKey(a), sessionTreeNodeKey(b))
  )
}

function stamp(value: Date | string | null | undefined): number {
  if (!value) return 0
  const at = typeof value === `string` ? new Date(value) : value
  const ms = at.getTime()
  return Number.isNaN(ms) ? 0 : ms
}

const compare = (a: string, b: string): number => (a < b ? -1 : a > b ? 1 : 0)

/** The top of `id`'s parent walk, or null when it is already a root. Breaks a
 *  cycle by returning null, so the row stays where it is. */
function ancestor<T extends SessionTreeRow>(
  id: string,
  parentOf: ReadonlyMap<string, string>,
  nodes: ReadonlyMap<string, SessionNode<T>>
): string | null {
  const parent = parentOf.get(id)
  if (!parent || !nodes.has(parent)) return null
  const seen = new Set<string>([id])
  let cursor: string | undefined = parent
  while (cursor) {
    if (seen.has(cursor)) return null
    seen.add(cursor)
    cursor = parentOf.get(cursor)
  }
  return parent
}

/** Children keep CREATION order, ties on the node's key. */
function byCreation<T extends SessionTreeRow>(
  a: SessionTreeNode<T>,
  b: SessionTreeNode<T>
): number {
  return (
    stamp(a.session.createdAt) - stamp(b.session.createdAt) ||
    compare(sessionTreeNodeKey(a), sessionTreeNodeKey(b))
  )
}

/** A node's activity counts its whole subtree's. */
function rollUp<T extends SessionTreeRow>(node: SessionTreeNode<T>): number {
  for (const child of node.children) {
    node.lastActivityAt = Math.max(node.lastActivityAt, rollUp(child))
  }
  return node.lastActivityAt
}

/** A node's stable identity — the key a collapsed set and a React list use. */
export function sessionTreeNodeKey<T extends SessionTreeRow>(
  node: SessionTreeNode<T>
): string {
  return node.session.id
}

/** One row of a DRAWN session tree: a node, how deep it sits and whether it
 *  can fold. Mirrored ×4 with `sessionTree` itself — every client flattens the
 *  tree the same way before it paints the EXP-965 connector over the depths. */
export interface SessionTreeFlatRow<T extends SessionTreeRow = SessionTreeRow> {
  node: SessionTreeNode<T>
  key: string
  depth: number
  hasChildren: boolean
}

/** The tree flattened top to bottom, skipping everything under a COLLAPSED
 *  node (keyed by `sessionTreeNodeKey`). */
export function visibleSessionTreeRows<T extends SessionTreeRow>(
  nodes: readonly SessionTreeNode<T>[],
  collapsed: ReadonlySet<string> = new Set<string>()
): SessionTreeFlatRow<T>[] {
  const out: SessionTreeFlatRow<T>[] = []
  const walk = (list: readonly SessionTreeNode<T>[], depth: number) => {
    for (const node of list) {
      const key = sessionTreeNodeKey(node)
      out.push({ node, key, depth, hasChildren: node.children.length > 0 })
      if (!collapsed.has(key)) walk(node.children, depth + 1)
    }
  }
  walk(nodes, 0)
  return out
}

/** Every session node of the tree, depth-first. */
export function flattenSessionTree<T extends SessionTreeRow>(
  nodes: readonly SessionTreeNode<T>[]
): SessionNode<T>[] {
  const out: SessionNode<T>[] = []
  const walk = (list: readonly SessionTreeNode<T>[]) => {
    for (const node of list) {
      out.push(node)
      walk(node.children)
    }
  }
  walk(nodes)
  return out
}

/** The ids of every row (resume successions included) nested at any depth
 *  under the node whose succession holds `sessionId`. */
export function sessionDescendantIds<T extends SessionTreeRow>(
  nodes: readonly SessionTreeNode<T>[],
  sessionId: string
): string[] {
  const find = (list: readonly SessionTreeNode<T>[]): SessionNode<T> | null => {
    for (const node of list) {
      if (node.chain.some((row) => row.id === sessionId)) return node
      const hit = find(node.children)
      if (hit) return hit
    }
    return null
  }
  const node = find(nodes)
  if (!node) return []
  return flattenSessionTree(node.children).flatMap((child) =>
    child.chain.map((row) => row.id)
  )
}
