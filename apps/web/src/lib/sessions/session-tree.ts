// EXP-1029 contract — the session tree (EXP-996 implements).
//
// ONE selector over the synced `coding_sessions` rows that every sessions
// list draws from: the web sidebar's Running section + `session-tree.tsx`,
// the IDE `sidebar.rs` Running section and its sessions list, the mobile
// sessions screens (EXP-996 declares the Rust/Swift/Kotlin mirrors itself —
// nobody else consumes them). Rules, in this order:
//
//   1. Resumed runs COLLAPSE: every resume succession (`runChain`, EXP-974)
//      is ONE node, keyed by its newest row, `chain` oldest-first.
//   2. Children nest under their `parentSessionId` (a `sessions_start`
//      child, EXP-679/818), following the parent's resume succession.
//   3. Sessions of ONE workflow group under `{ kind: 'workflow' }` — a row
//      belongs to the workflow whose `workflow_nodes` name it (by session id,
//      by issue, or by an issue a batch row covers) AND which the caller
//      listed in `workflows`, because that row is where the group's NAME comes
//      from. `startedReason: 'workflow'` alone never groups: it says a run is
//      SOME workflow's node, not which, and a group row cannot be drawn
//      without a name. (EXP-1029 wrote the looser rule; every client
//      implements this one.)
//   4. A stack (`issues.pr_base_branch` edges, `lib/pr-stack.ts`) groups
//      under `{ kind: 'stack', rootIssueId }` in LINEAR order, lowest first.
//      Stacks and workflows are NOT unified (decision 5): a stack is a
//      linear group with its own icon.
//   5. Groups and top-level nodes sort by last activity, newest first;
//      children keep creation order.
//   6. An orphan child whose parent is gone (swept, or not synced) sits at
//      top level.
//
// This file is the CONTRACT: the types and a stub. `session-tree.test.ts`
// carries the rules as a skipped table EXP-996 un-skips.
import type { CodingSession } from "@/db/schema"
import { stackChain, type PrStackNode } from "@/lib/pr-stack"
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
  > &
  // EXP-1082 §1: the server-stamped workflow membership. DECLARED only:
  // EXP-1068 groups by `workflowId` first (the skipped table in
  // session-tree.test.ts); optional so callers' rows need not carry it yet.
  Partial<Pick<CodingSession, `workflowId` | `workflowNodeId` | `workflowRole`>>

/** What the rows alone cannot say: which workflow an issue belongs to, and
 *  which issues stack on which. Every list is optional — a caller with no
 *  workflows synced still gets the session/parent tree. */
export interface SessionTreeContext<I extends PrStackNode = PrStackNode> {
  workflows?: readonly { id: string; name: string; status: string }[]
  /** `workflow_nodes` rows: which issue sits in which workflow. */
  workflowNodes?: readonly { workflowId: string; issueId: string; sessionId?: string | null }[]
  /** The issues the sessions name, for the stack edges. */
  issues?: readonly I[]
}

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

export interface WorkflowGroupNode<T extends SessionTreeRow = SessionTreeRow> {
  kind: `workflow`
  workflowId: string
  name: string
  /** The node runs, newest first. */
  children: SessionTreeNode<T>[]
  lastActivityAt: number
}

export interface StackGroupNode<T extends SessionTreeRow = SessionTreeRow> {
  kind: `stack`
  /** The lowest issue of the stack. */
  rootIssueId: string
  /** LINEAR: lowest first, one session node per stacked issue. */
  children: SessionTreeNode<T>[]
  lastActivityAt: number
}

export type SessionTreeNode<T extends SessionTreeRow = SessionTreeRow> =
  | SessionNode<T>
  | WorkflowGroupNode<T>
  | StackGroupNode<T>

/**
 * The sessions list as a tree. Pure: no clock, no IO; sort ties break on
 * id so two clients agree.
 */
export function sessionTree<T extends SessionTreeRow>(
  sessions: readonly T[],
  context: SessionTreeContext = {}
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
    // Rule 6: a parent that is gone (swept, another team, not synced) leaves
    // the child at top level; so does a row naming itself.
    if (parent && parent !== canonicalId) parentOf.set(canonicalId, parent)
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

  // 3. Children keep CREATION order (rule 5); a parent's activity counts its
  //    subtree's, so folding one never moves it.
  for (const node of nodes.values()) node.children.sort(byCreation)
  for (const node of roots) rollUp(node)

  // 4. Workflow groups, then stack groups — over the TOP-LEVEL nodes only
  //    (a child run stays under its parent wherever the parent lands).
  const grouped = groupStacks(groupWorkflows(roots, context), context)

  // 5. Groups and lone nodes sort by last activity, newest first.
  return grouped.sort(
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

/** Rule 5: children keep CREATION order, ties on the node's key. */
function byCreation<T extends SessionTreeRow>(
  a: SessionTreeNode<T>,
  b: SessionTreeNode<T>
): number {
  const created = (node: SessionTreeNode<T>) =>
    node.kind === `session` ? stamp(node.session.createdAt) : 0
  return (
    created(a) - created(b) ||
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

/** Rule 3: the sessions of ONE workflow under one group row. A node belongs
 *  to the workflow that lists its issue (or the node run itself) — the name
 *  comes from the `workflows` list, so a workflow the caller did not sync
 *  leaves its runs ungrouped. */
function groupWorkflows<T extends SessionTreeRow>(
  roots: readonly SessionNode<T>[],
  context: SessionTreeContext
): SessionTreeNode<T>[] {
  const workflows = new Map(
    (context.workflows ?? []).map((workflow) => [workflow.id, workflow])
  )
  if (workflows.size === 0 || !context.workflowNodes?.length) return [...roots]
  const byIssue = new Map<string, string>()
  const bySession = new Map<string, string>()
  for (const entry of context.workflowNodes) {
    if (!workflows.has(entry.workflowId)) continue
    if (entry.issueId && !byIssue.has(entry.issueId)) {
      byIssue.set(entry.issueId, entry.workflowId)
    }
    if (entry.sessionId && !bySession.has(entry.sessionId)) {
      bySession.set(entry.sessionId, entry.workflowId)
    }
  }
  const workflowOf = (node: SessionNode<T>): string | undefined => {
    for (const row of node.chain) {
      const named = bySession.get(row.id)
      if (named) return named
      if (row.issueId) {
        const byIssueId = byIssue.get(row.issueId)
        if (byIssueId) return byIssueId
      }
      // A batch node run covers several workflow issues (EXP-978).
      for (const issueId of row.batchIssueIds ?? []) {
        const covered = byIssue.get(issueId)
        if (covered) return covered
      }
    }
    return undefined
  }

  const out: SessionTreeNode<T>[] = []
  const groups = new Map<string, WorkflowGroupNode<T>>()
  for (const node of roots) {
    const workflowId = workflowOf(node)
    const workflow = workflowId ? workflows.get(workflowId) : undefined
    if (!workflowId || !workflow) {
      out.push(node)
      continue
    }
    let group = groups.get(workflowId)
    if (!group) {
      group = {
        kind: `workflow`,
        workflowId,
        name: workflow.name,
        children: [],
        lastActivityAt: 0,
      }
      groups.set(workflowId, group)
      out.push(group)
    }
    group.children.push(node)
    group.lastActivityAt = Math.max(group.lastActivityAt, node.lastActivityAt)
  }
  // The node runs, newest first.
  for (const group of groups.values()) {
    group.children.sort(
      (a, b) =>
        b.lastActivityAt - a.lastActivityAt ||
        compare(sessionTreeNodeKey(a), sessionTreeNodeKey(b))
    )
  }
  return out
}

/** Rule 4: a stack (`issues.pr_base_branch`) under one group row in LINEAR
 *  order, lowest first. A stack with only ONE of its runs listed is no group
 *  — the lone node stays where it was. */
function groupStacks<T extends SessionTreeRow>(
  entries: readonly SessionTreeNode<T>[],
  context: SessionTreeContext
): SessionTreeNode<T>[] {
  const issues = context.issues ?? []
  if (issues.length === 0) return [...entries]
  const byIssueId = new Map<string, PrStackNode>(
    issues.map((issue) => [issue.id, issue])
  )
  /** Every top-level session node that names an issue, by issue id. */
  const nodeOfIssue = new Map<string, SessionNode<T>>()
  for (const entry of entries) {
    if (entry.kind !== `session`) continue
    const issueId = entry.session.issueId
    if (issueId && byIssueId.has(issueId) && !nodeOfIssue.has(issueId)) {
      nodeOfIssue.set(issueId, entry)
    }
  }

  const out: SessionTreeNode<T>[] = []
  const claimed = new Set<string>()
  for (const entry of entries) {
    if (entry.kind !== `session`) {
      out.push(entry)
      continue
    }
    // A node already pulled into a group below is gone from the top level;
    // one that names no issue (a chat, an action, a batch) can be in no stack
    // and simply stays where it was.
    if (claimed.has(entry.session.id)) continue
    const issueId = entry.session.issueId
    const issue = issueId ? byIssueId.get(issueId) : undefined
    if (!issue) {
      out.push(entry)
      continue
    }
    const chain = stackChain(issue, issues)
    const members = chain.flatMap((member) => {
      const node = nodeOfIssue.get(member.id)
      return node && !claimed.has(node.session.id) ? [node] : []
    })
    if (chain.length < 2 || members.length < 2) {
      out.push(entry)
      continue
    }
    for (const member of members) claimed.add(member.session.id)
    out.push({
      kind: `stack`,
      rootIssueId: chain[0]!.id,
      children: members,
      lastActivityAt: Math.max(
        ...members.map((member) => member.lastActivityAt),
        0
      ),
    })
  }
  return out
}

/** A node's stable identity — the key a collapsed set and a React list use. */
export function sessionTreeNodeKey<T extends SessionTreeRow>(
  node: SessionTreeNode<T>
): string {
  if (node.kind === `session`) return node.session.id
  if (node.kind === `workflow`) return `workflow:${node.workflowId}`
  return `stack:${node.rootIssueId}`
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
 *  node (keyed by `sessionTreeNodeKey`). A group row with no children left is
 *  dropped: a group is its children. */
export function visibleSessionTreeRows<T extends SessionTreeRow>(
  nodes: readonly SessionTreeNode<T>[],
  collapsed: ReadonlySet<string> = new Set<string>()
): SessionTreeFlatRow<T>[] {
  const out: SessionTreeFlatRow<T>[] = []
  const walk = (list: readonly SessionTreeNode<T>[], depth: number) => {
    for (const node of list) {
      const key = sessionTreeNodeKey(node)
      if (node.kind !== `session` && node.children.length === 0) continue
      out.push({ node, key, depth, hasChildren: node.children.length > 0 })
      if (!collapsed.has(key)) walk(node.children, depth + 1)
    }
  }
  walk(nodes, 0)
  return out
}

/** Every session node of the tree, depth-first, groups flattened. */
export function flattenSessionTree<T extends SessionTreeRow>(
  nodes: readonly SessionTreeNode<T>[]
): SessionNode<T>[] {
  const out: SessionNode<T>[] = []
  const walk = (list: readonly SessionTreeNode<T>[]) => {
    for (const node of list) {
      if (node.kind === `session`) out.push(node)
      walk(node.children)
    }
  }
  walk(nodes)
  return out
}
