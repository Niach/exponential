import { and, eq, inArray, or, sql } from "drizzle-orm"
import type { db as database } from "@/db/connection"
import {
  boards,
  issueRelations,
  issues,
  workflowNodes,
  workflows,
} from "@/db/schema"
import type { WorkflowMetricsJson } from "@exp/db-schema/domain"
import { boardVisible } from "@/lib/board-visibility"
import { layoutWorkflow, type LayoutEdge } from "@/lib/workflow-layout"

// EXP-981: the server half of a workflow's GRAPH. The edges are the `blocks`
// relations among the covered issues and are never copied; what is stored is
// the derived picture — each node's `wave`/`lane`/`on_cycle` and the
// workflow's `metrics` — recomputed HERE every time the graph can have moved
// (create, a membership change, a `blocks`/`parent` relation written or
// removed on a covered issue). Clients draw it; none of them lays out.

type Tx = Parameters<Parameters<typeof database.transaction>[0]>[0]
type Executor = typeof database | Tx

/** `exp/wf-<id8>`: the integration branch of a workflow. */
export function workflowIntegrationBranch(workflowId: string): string {
  return `exp/wf-${workflowId.replace(/-/g, ``).slice(0, 8)}`
}

/** `exp/wf-<id8>-review-<IDENT>-r<round>`: the branch ONE agent review of a
 *  node runs on (EXP-984; mirror of `crates/coding` `review_branch`). The
 *  round follows the identifier, so `EXP-10`'s branches never match
 *  `EXP-103`'s. */
export function workflowReviewBranch(
  workflowId: string,
  identifier: string,
  round: number
): string {
  return `${workflowIntegrationBranch(workflowId)}-review-${identifier}-r${round}`
}

/** FEED-51: is `branch` one of this node's review branches, any round? The
 *  engine follows a reviewer by its branch rather than the session id it
 *  recorded (a resume mints a new id on the SAME branch), and the review
 *  gate accepts the same evidence. */
export function isWorkflowReviewBranch(
  workflowId: string,
  identifier: string,
  branch: string
): boolean {
  const prefix = workflowReviewBranch(workflowId, identifier, 0).slice(0, -1)
  return branch.startsWith(prefix) && /^[0-9]+$/.test(branch.slice(prefix.length))
}

const CLOSED_ANCHORS = new Set<string>([`done`, `cancelled`, `duplicate`])

/** The statuses a workflow still re-shapes itself in. A running workflow's
 *  node SET is the engine's business (dynamic admission, P5); its layout is
 *  still refreshed. */
const RESHAPED = new Set<string>([`draft`])

export interface WorkflowGraphNode {
  id: string
  issueId: string
  memberIssueIds: string[]
}

/**
 * Fold the picked issues into NODES: an issue whose ancestor (via `parent`)
 * is also picked joins that ancestor's compound node, and the OPEN sub-issues
 * of a picked issue join it even when nobody picked them (the planner splits
 * an issue after the workflow exists). Pure; `parentOf` is child → parent.
 */
export function foldCompoundNodes(
  pickedIds: readonly string[],
  parentOf: ReadonlyMap<string, string>,
  openIds: ReadonlySet<string>
): Array<{ issueId: string; memberIssueIds: string[] }> {
  const picked = new Set(pickedIds)
  const rootOf = (id: string): string => {
    let root = id
    const seen = new Set<string>([id])
    let cursor = parentOf.get(id)
    while (cursor !== undefined && !seen.has(cursor)) {
      seen.add(cursor)
      if (picked.has(cursor)) root = cursor
      cursor = parentOf.get(cursor)
    }
    // A parent CYCLE (legacy rows; EXP-980 refuses new ones) folds nothing:
    // each member would otherwise claim the other.
    if (cursor === id) return id
    return root
  }
  const members = new Map<string, Set<string>>()
  for (const id of pickedIds) {
    const root = rootOf(id)
    if (!members.has(root)) members.set(root, new Set())
    if (root !== id) members.get(root)!.add(id)
  }
  // Unpicked open descendants of a node's issue.
  const childrenOf = new Map<string, string[]>()
  for (const [child, parent] of parentOf) {
    childrenOf.set(parent, [...(childrenOf.get(parent) ?? []), child])
  }
  for (const [root, set] of members) {
    const stack = [root, ...set]
    const seen = new Set(stack)
    while (stack.length > 0) {
      for (const child of childrenOf.get(stack.pop()!) ?? []) {
        if (seen.has(child)) continue
        seen.add(child)
        // Another node's own subtree stays its own.
        if (members.has(child)) continue
        if (!openIds.has(child) && !picked.has(child)) continue
        set.add(child)
        stack.push(child)
      }
    }
  }
  return [...members].map(([issueId, set]) => ({
    issueId,
    memberIssueIds: [...set].sort(),
  }))
}

/** The `blocks` edges among nodes: a relation between ANY two covered issues
 *  of two different nodes. */
export function nodeEdges(
  nodes: readonly WorkflowGraphNode[],
  blocks: ReadonlyArray<{ issueId: string; relatedIssueId: string }>
): LayoutEdge[] {
  const nodeOf = new Map<string, string>()
  for (const node of nodes) {
    nodeOf.set(node.issueId, node.id)
    for (const member of node.memberIssueIds) nodeOf.set(member, node.id)
  }
  const edges: LayoutEdge[] = []
  for (const relation of blocks) {
    const from = nodeOf.get(relation.issueId)
    const to = nodeOf.get(relation.relatedIssueId)
    if (!from || !to || from === to) continue
    edges.push([from, to])
  }
  return edges
}

/** All descendants (any depth) of `rootIds` via `parent` rows, as child →
 *  parent. Bounded: a sub-issue tree is shallow, the walk stops at 12 levels. */
async function loadParentMap(
  executor: Executor,
  rootIds: readonly string[]
): Promise<Map<string, string>> {
  const parentOf = new Map<string, string>()
  let frontier = [...new Set(rootIds)]
  for (let depth = 0; depth < 12 && frontier.length > 0; depth += 1) {
    const rows = await executor
      .select({
        parent: issueRelations.issueId,
        child: issueRelations.relatedIssueId,
      })
      .from(issueRelations)
      .where(
        and(
          eq(issueRelations.type, `parent`),
          or(
            inArray(issueRelations.issueId, frontier),
            inArray(issueRelations.relatedIssueId, frontier)
          )
        )
      )
    const next: string[] = []
    for (const row of rows) {
      if (parentOf.has(row.child)) continue
      parentOf.set(row.child, row.parent)
      next.push(row.child)
    }
    frontier = next
  }
  return parentOf
}

/**
 * Re-derive one workflow's picture and write what changed. For a DRAFT the
 * node set is re-folded first (compound nodes follow the `parent` relations);
 * for every status the layout + metrics are refreshed. Returns the metrics.
 */
export async function replanWorkflow(
  tx: Tx,
  workflowId: string
): Promise<WorkflowMetricsJson | null> {
  const [workflow] = await tx
    .select({
      id: workflows.id,
      teamId: workflows.teamId,
      status: workflows.status,
      repositoryId: workflows.repositoryId,
    })
    .from(workflows)
    .where(eq(workflows.id, workflowId))
    .limit(1)
  if (!workflow) return null

  let nodes = await tx
    .select({
      id: workflowNodes.id,
      issueId: workflowNodes.issueId,
      memberIssueIds: workflowNodes.memberIssueIds,
    })
    .from(workflowNodes)
    .where(eq(workflowNodes.workflowId, workflowId))

  if (RESHAPED.has(workflow.status) && nodes.length > 0) {
    // Everything the workflow covers is re-folded from scratch, so a sub-issue
    // whose `parent` link was REMOVED comes back as a node of its own instead
    // of silently leaving the workflow with its old compound node.
    const picked = [
      ...new Set(nodes.flatMap((node) => [node.issueId, ...node.memberIssueIds])),
    ]
    const parentOf = await loadParentMap(tx, picked)
    const candidates = [...new Set([...parentOf.keys(), ...parentOf.values()])]
    const open = candidates.length
      ? await tx
          .select({
            id: issues.id,
            status: issues.status,
            repositoryId: boards.repositoryId,
          })
          .from(issues)
          .innerJoin(boards, eq(boards.id, issues.boardId))
          .where(and(inArray(issues.id, candidates), boardVisible()))
      : []
    // Adoption follows the rule every pick obeys (`loadPickableIssues`): a
    // workflow covers ONE repository, so an open sub-issue on a board of
    // another repository (or of none) stays outside.
    const openIds = new Set(
      open
        .filter(
          (row) =>
            !CLOSED_ANCHORS.has(row.status) &&
            row.repositoryId !== null &&
            row.repositoryId === workflow.repositoryId
        )
        .map((row) => row.id)
    )
    const folded = foldCompoundNodes(picked, parentOf, openIds)
    const keep = new Map(folded.map((node) => [node.issueId, node]))
    const gone = nodes.filter((node) => !keep.has(node.issueId))
    if (gone.length > 0) {
      await tx.delete(workflowNodes).where(
        inArray(
          workflowNodes.id,
          gone.map((node) => node.id)
        )
      )
    }
    nodes = nodes.filter((node) => keep.has(node.issueId))
    for (const node of nodes) {
      const next = keep.get(node.issueId)!
      if (next.memberIssueIds.join() === [...node.memberIssueIds].sort().join()) continue
      await tx
        .update(workflowNodes)
        .set({ memberIssueIds: next.memberIssueIds })
        .where(eq(workflowNodes.id, node.id))
      node.memberIssueIds = next.memberIssueIds
    }
    const have = new Set(nodes.map((node) => node.issueId))
    const fresh = folded.filter((node) => !have.has(node.issueId))
    if (fresh.length > 0) {
      const inserted = await tx
        .insert(workflowNodes)
        .values(
          fresh.map((node) => ({
            workflowId,
            teamId: workflow.teamId,
            issueId: node.issueId,
            memberIssueIds: node.memberIssueIds,
          }))
        )
        .returning({
          id: workflowNodes.id,
          issueId: workflowNodes.issueId,
          memberIssueIds: workflowNodes.memberIssueIds,
        })
      nodes = [...nodes, ...inserted]
    }
  }

  let covered = [
    ...new Set(nodes.flatMap((node) => [node.issueId, ...node.memberIssueIds])),
  ]
  const identifiers = covered.length
    ? await tx
        .select({ id: issues.id, identifier: issues.identifier })
        .from(issues)
        .where(inArray(issues.id, covered))
    : []
  const identifierOf = new Map(identifiers.map((row) => [row.id, row.identifier]))
  // A deleted MEMBER issue leaves no FK behind (`member_issue_ids` is json):
  // drop it from its node at every status, or the graph keeps counting it.
  // (A deleted node issue takes its node with it: FK cascade.)
  for (const node of nodes) {
    const kept = node.memberIssueIds.filter((id) => identifierOf.has(id))
    if (kept.length === node.memberIssueIds.length) continue
    await tx
      .update(workflowNodes)
      .set({ memberIssueIds: kept })
      .where(eq(workflowNodes.id, node.id))
    node.memberIssueIds = kept
  }
  covered = covered.filter((id) => identifierOf.has(id))
  const blocks = covered.length
    ? await tx
        .select({
          issueId: issueRelations.issueId,
          relatedIssueId: issueRelations.relatedIssueId,
        })
        .from(issueRelations)
        .where(
          and(
            eq(issueRelations.type, `blocks`),
            inArray(issueRelations.issueId, covered),
            inArray(issueRelations.relatedIssueId, covered)
          )
        )
    : []

  const layout = layoutWorkflow(
    nodes.map((node) => ({
      id: node.id,
      key: identifierOf.get(node.issueId) ?? node.issueId,
    })),
    nodeEdges(nodes, blocks)
  )

  const current = await tx
    .select({
      id: workflowNodes.id,
      wave: workflowNodes.wave,
      lane: workflowNodes.lane,
      onCycle: workflowNodes.onCycle,
    })
    .from(workflowNodes)
    .where(eq(workflowNodes.workflowId, workflowId))
  for (const row of current) {
    const placement = layout.placements.get(row.id)
    if (!placement) continue
    if (
      placement.wave === row.wave &&
      placement.lane === row.lane &&
      placement.cycle === row.onCycle
    ) {
      continue
    }
    await tx
      .update(workflowNodes)
      .set({ wave: placement.wave, lane: placement.lane, onCycle: placement.cycle })
      .where(eq(workflowNodes.id, row.id))
  }

  const metrics: WorkflowMetricsJson = {
    ...layout.metrics,
    cycleEdges: [...layout.cycleEdges].sort(),
  }
  // EXP-1066: `metrics` holds the layout facts and nothing else — written
  // whole, so a key an older engine wrote does not survive the next replan.
  await tx.update(workflows).set({ metrics }).where(eq(workflows.id, workflowId))
  return metrics
}

/**
 * A `blocks`/`parent` relation on these issues was written or removed:
 * re-derive every live workflow that covers one of them. Cheap when none
 * does (one indexed probe).
 */
export async function replanWorkflowsForIssues(
  tx: Tx,
  issueIds: readonly string[]
): Promise<void> {
  for (const workflowId of await workflowIdsCoveringIssues(tx, issueIds)) {
    await replanWorkflow(tx, workflowId)
  }
}

/**
 * The live (draft/running/paused) workflows covering any of these issues, as
 * node issue or compound member. Read it BEFORE deleting an issue: the FK
 * cascade takes the node with the issue, so afterwards nothing points back at
 * the workflow that has to re-lay out (`replanWorkflow` each id after the
 * delete).
 */
export async function workflowIdsCoveringIssues(
  executor: Executor,
  issueIds: readonly string[]
): Promise<string[]> {
  const ids = [...new Set(issueIds)]
  if (ids.length === 0) return []
  const rows = await executor
    .select({ workflowId: workflowNodes.workflowId })
    .from(workflowNodes)
    .innerJoin(workflows, eq(workflows.id, workflowNodes.workflowId))
    .where(
      and(
        inArray(workflows.status, [`draft`, `running`, `paused`]),
        or(
          inArray(workflowNodes.issueId, ids),
          sql`${workflowNodes.memberIssueIds} ?| ${sql.param(ids)}::text[]`
        )
      )
    )
  return [...new Set(rows.map((row) => row.workflowId))]
}

/**
 * EXP-982: is this issue covered by a node of a workflow that is still
 * running (or paused)? Such an issue's PR merges into the workflow's
 * integration branch, so the merge must not move it to the team's PR-merge
 * status yet — the final PR's merge does.
 */
export async function issueLandsInLiveWorkflow(
  executor: Executor,
  issueId: string
): Promise<boolean> {
  const rows = await executor
    .select({ id: workflowNodes.id })
    .from(workflowNodes)
    .innerJoin(workflows, eq(workflows.id, workflowNodes.workflowId))
    .where(
      and(
        inArray(workflows.status, [`running`, `paused`]),
        or(
          eq(workflowNodes.issueId, issueId),
          sql`${workflowNodes.memberIssueIds} ? ${issueId}`
        )
      )
    )
    .limit(1)
  return rows.length > 0
}

/** EXP-1010: remember the base a live node's PR merged into. NULL (a base
 *  nobody recorded) reads as the integration branch in `landNode`. */
export async function stampNodeMergedInto(
  executor: Executor,
  issueId: string,
  base: string | null
): Promise<void> {
  const live = executor
    .select({ id: workflows.id })
    .from(workflows)
    .where(inArray(workflows.status, [`running`, `paused`]))
  await executor
    .update(workflowNodes)
    .set({ mergedInto: base })
    .where(and(eq(workflowNodes.issueId, issueId), inArray(workflowNodes.workflowId, live)))
}

/** The live workflow node covering an issue, with what `pr_open` needs to
 *  derive the base: agents pass nothing new. */
export async function liveWorkflowBaseForIssue(
  executor: Executor,
  issueId: string
): Promise<{ workflowId: string; nodeId: string; base: string } | null> {
  const [row] = await executor
    .select({
      workflowId: workflows.id,
      nodeId: workflowNodes.id,
      baseBranch: workflowNodes.baseBranch,
      integrationBranch: workflows.integrationBranch,
    })
    .from(workflowNodes)
    .innerJoin(workflows, eq(workflows.id, workflowNodes.workflowId))
    .where(
      and(
        inArray(workflows.status, [`running`, `paused`]),
        or(
          eq(workflowNodes.issueId, issueId),
          sql`${workflowNodes.memberIssueIds} ? ${issueId}`
        )
      )
    )
    .limit(1)
  if (!row) return null
  return {
    workflowId: row.workflowId,
    nodeId: row.nodeId,
    base: row.baseBranch ?? row.integrationBranch,
  }
}

/** A workflow's nodes and the `blocks` edges between them (EXP-983): what the
 *  merge train's order and the post-landing retarget are decided on. A
 *  `proposed` node was never admitted, so it is ABSENT here, edges included:
 *  the engine drops it the same way, and a covered node must never wait on
 *  an outsider a member has not let in. */
export async function loadWorkflowEdges(executor: Executor, workflowId: string) {
  const nodes = (
    await executor
      .select({
        id: workflowNodes.id,
        issueId: workflowNodes.issueId,
        memberIssueIds: workflowNodes.memberIssueIds,
        state: workflowNodes.state,
        baseBranch: workflowNodes.baseBranch,
        // EXP-1103: the review-wave gate reads the layer and the stamp.
        wave: workflowNodes.wave,
        approvedAt: workflowNodes.approvedAt,
      })
      .from(workflowNodes)
      .where(eq(workflowNodes.workflowId, workflowId))
  ).filter((node) => node.state !== `proposed`)
  const covered = nodes.flatMap((node) => [node.issueId, ...node.memberIssueIds])
  const blocks = covered.length
    ? await executor
        .select({
          issueId: issueRelations.issueId,
          relatedIssueId: issueRelations.relatedIssueId,
        })
        .from(issueRelations)
        .where(
          and(
            eq(issueRelations.type, `blocks`),
            inArray(issueRelations.issueId, covered),
            inArray(issueRelations.relatedIssueId, covered)
          )
        )
    : []
  return { nodes, edges: nodeEdges(nodes, blocks) }
}

/**
 * EXP-984: dynamic graphs. A `blocks` relation was written between an issue a
 * LIVE workflow covers and one it does not. If the outsider is a follow-up
 * filed DURING the run (created after the workflow started), still in the
 * backlog and in the workflow's repository, it arrives as a node:
 *
 * - additive → admitted at once (`blocked`): it sits DOWNSTREAM of the covered
 *   issue (covered blocks new) and blocks nothing the workflow covers, so no
 *   running node's base changes and no cycle is possible;
 * - anything else (it blocks covered work) → `proposed`: the engine ignores it
 *   until a member admits or dismisses it.
 *
 * Returns the workflow ids touched. Pure insert; the caller replans.
 */
export async function proposeNodesForRelation(
  tx: Tx,
  relation: { issueId: string; relatedIssueId: string }
): Promise<string[]> {
  const ends = [relation.issueId, relation.relatedIssueId]
  const covering = await tx
    .select({
      workflowId: workflows.id,
      teamId: workflows.teamId,
      repositoryId: workflows.repositoryId,
      startedAt: workflows.startedAt,
      issueId: workflowNodes.issueId,
      members: workflowNodes.memberIssueIds,
    })
    .from(workflowNodes)
    .innerJoin(workflows, eq(workflows.id, workflowNodes.workflowId))
    .where(
      and(
        inArray(workflows.status, [`running`, `paused`]),
        or(
          inArray(workflowNodes.issueId, ends),
          sql`${workflowNodes.memberIssueIds} ?| ${sql.param(ends)}::text[]`
        )
      )
    )
  const touched: string[] = []
  for (const workflowId of new Set(covering.map((row) => row.workflowId))) {
    const rows = covering.filter((row) => row.workflowId === workflowId)
    const workflow = rows[0]!
    const all = await tx
      .select({ issueId: workflowNodes.issueId, members: workflowNodes.memberIssueIds })
      .from(workflowNodes)
      .where(eq(workflowNodes.workflowId, workflowId))
    const covered = new Set(all.flatMap((row) => [row.issueId, ...row.members]))
    const outsiders = ends.filter((id) => !covered.has(id))
    if (outsiders.length !== 1 || !workflow.startedAt) continue
    const outsider = outsiders[0]!
    const [issue] = await tx
      .select({
        status: issues.status,
        createdAt: issues.createdAt,
        repositoryId: boards.repositoryId,
      })
      .from(issues)
      .innerJoin(boards, eq(boards.id, issues.boardId))
      .where(and(eq(issues.id, outsider), boardVisible()))
      .limit(1)
    if (
      !issue ||
      issue.status !== `backlog` ||
      issue.repositoryId !== workflow.repositoryId ||
      new Date(issue.createdAt).getTime() < new Date(workflow.startedAt).getTime()
    ) {
      continue
    }
    // Additive = the outsider is the BLOCKED end and blocks nothing covered.
    const blocksCovered = await tx
      .select({ id: issueRelations.id })
      .from(issueRelations)
      .where(
        and(
          eq(issueRelations.type, `blocks`),
          eq(issueRelations.issueId, outsider),
          inArray(issueRelations.relatedIssueId, [...covered])
        )
      )
      .limit(1)
    const additive = relation.relatedIssueId === outsider && blocksCovered.length === 0
    await tx
      .insert(workflowNodes)
      .values({
        workflowId,
        teamId: workflow.teamId,
        issueId: outsider,
        state: additive ? `blocked` : `proposed`,
        note: additive ? null : `Filed during the run; it changes what existing nodes wait for`,
      })
      .onConflictDoNothing()
    touched.push(workflowId)
  }
  return touched
}
