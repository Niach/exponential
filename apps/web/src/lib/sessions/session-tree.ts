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
//      whose issue is a `workflow_nodes` row of a running workflow (or whose
//      `startedReason` is `workflow`) belongs to that workflow.
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
import type { PrStackNode } from "@/lib/pr-stack"
import type { SessionRow as RunChainRow } from "./run-chain"

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
  void sessions
  void context
  throw new Error(`EXP-996 implements lib/sessions/session-tree.ts`)
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
