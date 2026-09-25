// EXP-1029 contract — the session tree (EXP-996 implements, EXP-1068 groups
// by the server-stamped workflow membership).
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
//      child, EXP-679/818), following the parent's resume succession —
//      UNLESS the child belongs to a workflow the parent does not (EXP-1068:
//      a chat that resumed a node run stays in the workflow's group, not
//      under the chat; a child with no workflow of its own always nests).
//   3. Sessions of ONE workflow group under `{ kind: 'workflow' }` — a row
//      belongs to the workflow its `workflowId` names (EXP-1082 §1: stamped
//      ONCE on the server, inherited by resumes and children; a chain's
//      membership is its newest stamped row's). The group is drawn only when
//      the caller listed that workflow in `workflows`, because that row is
//      where the group's NAME comes from — nothing here invents a label. The
//      old heuristics (a `workflow_nodes` row naming the session, the issue,
//      or a batch's issues) are GONE: `startedReason: 'workflow'` alone never
//      groups, and neither does an unstamped row. Inside a group:
//        a. one row per node's AUTHOR chain (`workflowRole: author`);
//        b. a REVIEW chain nests under its node's author row — the live one,
//           else the newest — captioned by `reviewRowCaption` (`Review r2 ·
//           approved`), its round parsed off the review branch
//           (`reviewBranchRound`); a review whose node has no author listed
//           is a plain child of the group;
//        c. `base_merge`, `plan`, `replan` and any other stamped row are plain
//           children of the group;
//        d. a node with TWO live author chains or two live review chains is
//           the duplicate case (workflow 3b828f50's five reviewers): every
//           author row of that node carries `duplicateLive`, the warning.
//      The group row carries the workflow's status and the counts its caption
//      is made of (`workflowGroupCaption`: `3 running · 5 of 8 done`).
//   4. A stack (`issues.pr_base_branch` edges, `lib/pr-stack.ts`) groups
//      under `{ kind: 'stack', rootIssueId }` in LINEAR order, lowest first.
//      Stacks and workflows are NOT unified (decision 5): a stack is a
//      linear group with its own icon.
//   5. Groups and top-level nodes sort by last activity, newest first;
//      children keep creation order.
//   6. An orphan child whose parent is gone (swept, or not synced) sits at
//      top level.
//
// Mirrored ×4 with these test names: desktop `domain::session_tree`, iOS
// `SessionTree.swift`, Android `SessionTree.kt`. Every string a client draws
// off this module (`workflowGroupCaption`, `reviewRowCaption`) is
// byte-identical there.
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
  // EXP-1082 §1: the server-stamped workflow membership, which EXP-1068
  // groups by FIRST; `branch` carries a review run's round. Optional so a
  // caller's rows need not carry them (an unstamped row is simply ungrouped).
  Partial<
    Pick<CodingSession, `workflowId` | `workflowNodeId` | `workflowRole` | `branch`>
  >

/** What the rows alone cannot say: which workflow is called what, how its
 *  nodes stand, and which issues stack on which. Every list is optional — a
 *  caller with no workflows synced still gets the session/parent tree. */
export interface SessionTreeContext<I extends PrStackNode = PrStackNode> {
  workflows?: readonly { id: string; name: string; status: string }[]
  /** `workflow_nodes` rows. Since EXP-1068 they group NOTHING (the rows
   *  carry their membership); `state` (contract `wfNodeState`) feeds the
   *  group caption's `5 of 8 done`. */
  workflowNodes?: readonly {
    id?: string
    workflowId: string
    issueId: string
    sessionId?: string | null
    state?: string | null
  }[]
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
  /** EXP-1068 3b: a review run's round, off its branch (`exp/wf-<id8>-
   *  review-<IDENT>-r<n>`); null on every other row and on a review whose
   *  branch did not sync. */
  reviewRound: number | null
  /** EXP-1068 3d: this author row's node has two live author chains or two
   *  live review chains — the warning glyph. Never set on a review row. */
  duplicateLive: boolean
}

export interface WorkflowGroupNode<T extends SessionTreeRow = SessionTreeRow> {
  kind: `workflow`
  workflowId: string
  /** The workflow's synced name — never a hardcoded label (EXP-1068 rule 4). */
  name: string
  /** contract `wfStatus` — the group row's glyph. */
  status: string
  /** How many session nodes of the whole subtree are live (not `ended`). */
  liveRuns: number
  /** `workflow_nodes` in state `landed`, of `nodesTotal` — the caption. */
  nodesDone: number
  nodesTotal: number
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

/** A row is LIVE until the server ends it (`running` and `in_review` both
 *  are; `needs_input`/`blocked` are flags on a live row). */
export function sessionRowIsLive(row: Pick<SessionTreeRow, `status`>): boolean {
  return row.status !== `ended`
}

/** EXP-1068 3b: the round a review branch carries — `exp/wf-<id8>-review-
 *  <IDENT>-r<n>` → n. Null for any other branch. The rule the engine writes
 *  it with is `coding::workflows::review_branch`; only the SUFFIX is read
 *  here so the four clients cannot drift on the prefix. */
export function reviewBranchRound(branch: string | null | undefined): number | null {
  if (!branch) return null
  const match = /-r(\d+)$/.exec(branch)
  if (!match) return null
  const round = Number(match[1])
  return Number.isSafeInteger(round) && round > 0 ? round : null
}

/** What a review row says about its verdict. `submitted` = an older round
 *  whose verdict the node row no longer carries (only the latest is stored). */
export type ReviewRowVerdict = `approved` | `changes_requested` | `submitted` | `none`

/** EXP-1068 3b: the verdict word for the review of `round`, from the node's
 *  `review_round` (rounds submitted so far) and its latest `review`. */
export function reviewRoundVerdict(
  round: number | null,
  node: {
    reviewRound: number
    review: { round: number; verdict: string } | null | undefined
  } | null | undefined
): ReviewRowVerdict {
  if (round == null || !node) return `none`
  const latest = node.review
  if (latest && latest.round === round) {
    return latest.verdict === `approve` ? `approved` : `changes_requested`
  }
  return round <= node.reviewRound ? `submitted` : `none`
}

/** EXP-1068 3b: a review row's title. `Review r2 · approved`, `Review r2 ·
 *  changes requested`, `Review r2 · submitted`, `Review r2 · no verdict`
 *  (ended, nothing submitted), `Review r2` (still reviewing), `Review` (no
 *  round known). Byte-identical ×4. */
export function reviewRowCaption(
  round: number | null,
  verdict: ReviewRowVerdict,
  live: boolean
): string {
  const title = round == null ? `Review` : `Review r${round}`
  switch (verdict) {
    case `approved`:
      return `${title} · approved`
    case `changes_requested`:
      return `${title} · changes requested`
    case `submitted`:
      return `${title} · submitted`
    case `none`:
      return live ? title : `${title} · no verdict`
  }
}

/** EXP-1068 rule 3: the group row's trailing caption — `3 running · 5 of 8
 *  done`; `5 of 8 done` with nothing live; `3 running` before the nodes
 *  synced; empty with neither. Byte-identical ×4. */
export function workflowGroupCaption(
  group: Pick<WorkflowGroupNode, `liveRuns` | `nodesDone` | `nodesTotal`>
): string {
  const parts: string[] = []
  if (group.liveRuns > 0) parts.push(`${group.liveRuns} running`)
  if (group.nodesTotal > 0) parts.push(`${group.nodesDone} of ${group.nodesTotal} done`)
  return parts.join(` · `)
}

interface Membership {
  workflowId: string
  nodeId: string | null
  role: string | null
}

/** A chain's membership: its NEWEST stamped row's (a resume inherits the
 *  stamp, so the whole succession agrees; the newest wins if not). */
function membershipOf<T extends SessionTreeRow>(chain: readonly T[]): Membership | null {
  for (let index = chain.length - 1; index >= 0; index -= 1) {
    const row = chain[index]!
    if (row.workflowId) {
      return {
        workflowId: row.workflowId,
        nodeId: row.workflowNodeId ?? null,
        role: row.workflowRole ?? null,
      }
    }
  }
  return null
}

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
  const memberships = new Map<string, Membership | null>()
  for (const [canonicalId, chain] of chainOf) {
    memberships.set(canonicalId, membershipOf(chain))
  }

  // 2. Children nest under their parent's SUCCESSION (EXP-906: a resume
  //    inherits `parentSessionId`, so the whole chain answers for it) —
  //    unless the child is a workflow's and the parent is not that
  //    workflow's (EXP-1068: the group claims it).
  const parentOf = new Map<string, string>()
  for (const [canonicalId, chain] of chainOf) {
    let named: string | null = null
    for (let index = chain.length - 1; index >= 0 && !named; index -= 1) {
      named = chain[index]!.parentSessionId ?? null
    }
    const parent = named ? canonicalOf.get(named) : undefined
    // Rule 6: a parent that is gone (swept, another team, not synced) leaves
    // the child at top level; so does a row naming itself.
    if (!parent || parent === canonicalId) continue
    const own = memberships.get(canonicalId)
    const parents = memberships.get(parent)
    if (own && own.workflowId !== parents?.workflowId) continue
    parentOf.set(canonicalId, parent)
  }

  const nodes = new Map<string, SessionNode<T>>()
  for (const [canonicalId, chain] of chainOf) {
    const session = chain[chain.length - 1]!
    const membership = memberships.get(canonicalId)
    nodes.set(canonicalId, {
      kind: `session`,
      session,
      chain,
      children: [],
      lastActivityAt: Math.max(...chain.map((row) => stamp(row.updatedAt)), 0),
      reviewRound:
        membership?.role === `review` ? reviewBranchRound(session.branch) : null,
      duplicateLive: false,
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
  const grouped = groupStacks(
    groupWorkflows(roots, memberships, context),
    context
  )

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

/** Newest activity first, ties on the key — groups' children and the top. */
function byActivity<T extends SessionTreeRow>(
  a: SessionTreeNode<T>,
  b: SessionTreeNode<T>
): number {
  return (
    b.lastActivityAt - a.lastActivityAt ||
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

/** How many session nodes of a subtree are live. */
function liveCount<T extends SessionTreeRow>(nodes: readonly SessionTreeNode<T>[]): number {
  let count = 0
  for (const node of nodes) {
    if (node.kind === `session` && sessionRowIsLive(node.session)) count += 1
    count += liveCount(node.children)
  }
  return count
}

/** Rule 3: the sessions of ONE workflow under one group row, by the rows'
 *  own `workflowId`. The name comes from the `workflows` list, so a workflow
 *  the caller did not sync leaves its runs ungrouped. */
function groupWorkflows<T extends SessionTreeRow>(
  roots: readonly SessionNode<T>[],
  memberships: ReadonlyMap<string, Membership | null>,
  context: SessionTreeContext
): SessionTreeNode<T>[] {
  const workflows = new Map(
    (context.workflows ?? []).map((workflow) => [workflow.id, workflow])
  )
  if (workflows.size === 0) return [...roots]

  const out: SessionTreeNode<T>[] = []
  const groups = new Map<string, WorkflowGroupNode<T>>()
  /** Per group: the author chains and review chains by node id, and the
   *  rows that are neither (plain children). */
  const parts = new Map<
    string,
    {
      authors: Map<string, SessionNode<T>[]>
      reviews: Map<string, SessionNode<T>[]>
      plain: SessionNode<T>[]
    }
  >()
  for (const node of roots) {
    const membership = memberships.get(node.session.id)
    const workflow = membership ? workflows.get(membership.workflowId) : undefined
    if (!membership || !workflow) {
      out.push(node)
      continue
    }
    let group = groups.get(workflow.id)
    if (!group) {
      const nodesOf = (context.workflowNodes ?? []).filter(
        (entry) => entry.workflowId === workflow.id
      )
      group = {
        kind: `workflow`,
        workflowId: workflow.id,
        name: workflow.name,
        status: workflow.status,
        liveRuns: 0,
        nodesDone: nodesOf.filter((entry) => entry.state === `landed`).length,
        nodesTotal: nodesOf.length,
        children: [],
        lastActivityAt: 0,
      }
      groups.set(workflow.id, group)
      parts.set(workflow.id, { authors: new Map(), reviews: new Map(), plain: [] })
      out.push(group)
    }
    const part = parts.get(workflow.id)!
    const { nodeId, role } = membership
    if (nodeId && role === `author`) {
      push(part.authors, nodeId, node)
    } else if (nodeId && role === `review`) {
      push(part.reviews, nodeId, node)
    } else {
      part.plain.push(node)
    }
  }

  for (const [workflowId, group] of groups) {
    const part = parts.get(workflowId)!
    // 3a/3d: one row per author chain; the node's HEAD author is the live
    // one with the newest activity, else the newest.
    for (const [nodeId, authors] of part.authors) {
      authors.sort(
        (a, b) =>
          Number(sessionRowIsLive(b.session)) - Number(sessionRowIsLive(a.session)) ||
          byActivity(a, b)
      )
      const reviews = part.reviews.get(nodeId) ?? []
      const liveAuthors = authors.filter((node) => sessionRowIsLive(node.session)).length
      const liveReviews = reviews.filter((node) => sessionRowIsLive(node.session)).length
      const duplicate = liveAuthors > 1 || liveReviews > 1
      for (const author of authors) author.duplicateLive = duplicate
      // 3b: the reviews nest under the head author, in creation order with
      // its own children.
      const head = authors[0]!
      if (reviews.length > 0) {
        head.children.push(...reviews)
        head.children.sort(byCreation)
        rollUp(head)
      }
      part.reviews.delete(nodeId)
      group.children.push(...authors)
    }
    // 3b: a review whose node has no author listed is a plain child.
    for (const reviews of part.reviews.values()) group.children.push(...reviews)
    // 3c: everything else.
    group.children.push(...part.plain)
    // The node runs, newest first.
    group.children.sort(byActivity)
    group.lastActivityAt = Math.max(
      ...group.children.map((child) => child.lastActivityAt),
      0
    )
    group.liveRuns = liveCount(group.children)
  }
  return out
}

function push<K, V>(map: Map<K, V[]>, key: K, value: V): void {
  const list = map.get(key)
  if (list) list.push(value)
  else map.set(key, [value])
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
